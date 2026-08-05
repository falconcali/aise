# AISE Turn Runtime Spec 代码实现 Review 报告

> **审查对象**：`falconcali/aise` `main@bf653e5439ff04a563d50fb8be8f8492a6bd8bee`  
> **审查基线**：`doc/exec/2026-08004-Turn-Runtime-Codegen-Spec-gpt.md` v1.0（基线 `main@c14f84e`）  
> **审查日期**：2026-08-05  
> **覆盖范围**：Turn Core、Engine/Runtime、各业务 Pipeline、LLM Gateway/Provider、Persistence、SSE/Task 生命周期，以及对应测试；Prompt 资产管理子系统只检查与 Turn Runtime 的集成点。

## 1. 总体结论

**结论：当前实现不能判定为 Spec 最终完成，建议暂不进入新的业务增强阶段。**

核心架构方向是对的，而且主要骨架已经落地：Core Contract、固定 `TurnPipelineSet`、有界 Repair Loop、Story 级串行化、统一 `LlmGateway`、Proposal → Validation → Commit、revision CAS、幂等、事务和 Outbox 都已存在。当前 commit 的 GitHub Actions 也全部为绿色。

但代码仍存在若干会破坏 Spec 核心不变量的问题，主要集中在五个方面：

1. 幂等重放和部分前置错误不会向 SSE 客户端发送任何终态结果，响应丢失后的恢复链路实际上不可用。
2. `Cancelled`、`Conflict` 等终态没有真正写入 Context，LLM/Store 内层错误还会被错误分类为普通 `Failed`。
3. `ValidatedChangeSet` 的可信边界可被公开 API 绕过，Character Thought 也仍可能被模型转换为 World Fact 后通过校验。
4. Context、Snapshot、Proposal、Retrieval、任务等待队列和 Trace 持久化仍有未受控资源边界。
5. 生产启动流程没有接入 graceful shutdown；Snapshot/Baseline/Commit 也尚未覆盖故事指令、配置、当前场景、权威摘要和约束。

综合判断：**Phase 0 和 Phase 5 基本完成；Phase 1、2、4、6 部分完成；Phase 3 的安全边界与 Phase 7 的业务上下文仍未完成。**

## 2. 已正确完成、应当保留的部分

- 已建立 `core` Turn Contract 层，旧的 Runtime Contract 文件和 `StoryDraft` 路径已删除。
- 所有业务 Pipeline 均通过 `TurnExecutionPipeline::execute(&mut TurnExecutionContext)` 工作，业务 Pipeline 未直接依赖 `LlmProvider`。
- `TurnPipelineSet` 使用固定命名字段并校验 Stage，Runtime 顺序不可动态重排。
- Retrieval、Character Think 的跳过由 Runtime 控制，跳过时不会发送 `StageStarted`。
- Repair 预算在 Repairer 调用前消费，预算耗尽不会进入 Commit。
- Committer 同时检查 `ReadyToCommit` 和 `ValidatedChangeSet`，不会直接提交 `StoryProposal`。
- SQLite Commit 已具备事务、revision CAS、幂等键、原结果恢复和 Outbox 原子写入。
- Recent Turns 按时间正序返回，Player Character 使用稳定 ID 选择，`StateChange::Unchanged` 不会覆盖已有 World。
- `StoryTurnCoordinator` 已实现同 Story 串行、不同 Story 并行、等待上限和空闲回收。
- Spec 第 15 节列出的 **48 个测试名称全部存在**。
- 当前 commit 的 [GitHub Actions CI](https://github.com/falconcali/aise/actions/runs/31004219678) 成功完成 Format、Clippy、Check 和 Test。

## 3. 红线问题

| ID | 红线问题 | 级别 | 直接风险 |
| --- | --- | --- | --- |
| R1 | 幂等重放与前置错误没有统一终态发布 | P1 | HTTP 已返回 SSE 200，但客户端可能收到空流，无法知道成功、冲突还是失败 |
| R2 | 错误类型与 Context 终态不一致 | P1 | Cancel/Deadline/Revision Conflict 被当成普通失败，`Cancelled`/`Conflict` Phase 基本是死状态 |
| R3 | Validation 信任边界可绕过 | P1 | 外部代码可直接构造 `ValidatedChangeSet`；私有 Thought 可被提议为 World Fact 并提交 |
| R4 | 资源边界与 shutdown 链路不闭合 | P1 | 无界等待、锁跨 `.await`、服务退出时直接中止任务、Trace 文件无限增长 |
| R5 | Story 权威上下文未完整进入 Snapshot/Commit | P1 | Writer 实际拿不到故事指令、配置、权威场景、完整摘要和约束，生成质量与状态一致性都不可靠 |

## 4. 详细问题

### P1-01：幂等重放会返回空 SSE，恢复闭环未成立

**证据**：[`engine.rs:91-103`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/engine.rs#L91-L103) 在命中已提交幂等结果时直接 `return Ok(outcome.result)`，而所有 `Committed/Failed/Conflict/TraceCompleted` 事件只在 [`engine.rs:127-177`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/engine.rs#L127-L177) 的 Runtime 之后发布。HTTP Task 又只记录错误、忽略成功返回值（[`api/turn.rs:48-56`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise-server/src/api/turn.rs#L48-L56)）。

**结果**：

- 相同 key、相同请求的重试不会再次调用 LLM，但 SSE 客户端也收不到原 `CommittedTurnResult`。
- 相同 key、不同请求在 Context 创建前返回冲突，同样没有 `Conflict` 事件。
- 当前没有按 idempotency key 查询 Turn Result 的 GET API；SSE 终态丢失后没有其他恢复入口。

**建议**：Engine 所有退出路径必须经过统一 finalizer。幂等命中必须发布 `Committed(original_result)`；前置冲突需要可在 TurnIdentity 创建前表达的事件模型，或在建立 SSE 前完成同步 preflight。另增加按 `(story_id, idempotency_key)` 查询结果的恢复接口和端到端测试。

### P1-02：完整请求尚未校验就可能产生数据库副作用

**证据**：Engine 只先校验 `player_input`，随后使用尚未验证的 `story_id` 获取 permit、访问 Store，甚至自动创建 Story，最后才通过 `TurnIdentity::new` 检查 ID（[`engine.rs:96-119`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/engine.rs#L96-L119)）。`StoryId` 仍可通过 `StoryId::from("")` 构造。

**结果**：直接 Engine 调用传入空 `StoryId` 时，可能先写入一个空 ID 的 Story，再返回 `InvalidRequest`。不存在的 Story 也会被 Turn 执行入口静默创建，这不是 Spec 定义的 Turn 职责，并绕过了独立的 Story 创建流程。

**建议**：新增 `ExecuteTurnSpec::try_into_validated`，在任何 permit、Store 或 Trace 副作用前一次性验证 StoryId、IdempotencyKey、Request、deadline 配置和 TurnId。Story 创建应成为显式用例；若自动创建是新的产品决策，应先更新 Spec 和权威状态初始化规则。

### P1-03：错误被错误分类，Context 没有进入完整终态

**证据**：`AiseError` 使用 `Llm(#[from] LlmError)` 和 `Store(#[from] StoreError)` 包装内层错误（[`error.rs:1-42`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/error.rs#L1-L42)），但 Engine 只匹配顶层 `AiseError::Cancelled/RevisionConflict/IdempotencyConflict`（[`engine.rs:162-173`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/engine.rs#L162-L173)）。Context 只有 Validation Reject 会写 `Failed`，没有通用的 `mark_failed/mark_cancelled/mark_conflict`（[`turn_context.rs:258-282`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/core/turn_context.rs#L258-L282)）。

**结果**：

- LLM 调用期间取消会变成 `AiseError::Llm(LlmError::Cancelled)`，最终发布 `Failed` 而不是 `Cancelled`。
- Store 返回 `StoreError::RevisionConflict` 时会被发布为 `Failed` 而不是 `Conflict`。
- Repair 预算耗尽、Pipeline 错误、deadline 等失败后，Context 仍停留在中间 Phase。

**建议**：建立单一 `TurnFailureKind`/错误归一化层，并在 Engine finalizer 中无条件推进 Context 到唯一终态。增加 nested LLM cancel、nested Store conflict、repair exhaustion、pipeline failure 的 Phase 与事件断言。

### P1-04：Core 的依赖方向并未真正单向

**证据**：Core 文件直接使用 `crate::error::AiseError`，而 `AiseError` 又导入 `llm::LlmError` 和 `persistence::StoreError`。同时 `LlmError::Transport` 暴露 `reqwest::Error`，`StoreError::Database` 暴露 `sqlx::Error`（[`llm/error.rs`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/llm/error.rs)、[`persistence/store.rs:17-31`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/persistence/store.rs#L17-L31)）。

**结果**：逻辑依赖实际形成 `core -> AiseError -> llm/persistence` 的反向边；Provider/Gateway Error 和 Store Port/SQLite Adapter Error 也没有分层。当前依赖测试只扫描直接字符串，因此没有发现这条传递依赖。

**建议**：拆分 Core Turn Error、Gateway `LlmError`、Provider `LlmProviderError`、Store Port Error 和 SQLite Adapter Error；只在 Engine/Composition Root 做映射，Core Error 不得持有外层错误类型。

### P1-05：`ValidatedChangeSet` 不是封闭的可信类型

**证据**：`ValidatedChangeSet` 字段虽私有，但 `new` 是公开方法（[`turn_validation.rs:98-125`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/core/turn_validation.rs#L98-L125)），集成测试也在 Validation 之外直接构造它。`ValidationResult::pass().with_issue(fatal(...))` 也能构造“Pass + Fatal Issue”的矛盾对象（[`turn_validation.rs:20-45`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/core/turn_validation.rs#L20-L45)）。

**结果**：类型系统没有真正保证 ChangeSet 只能由合法 Validation 产生，也没有保证 Decision 与 Issues 一致。

**建议**：将 ChangeSet 构造器限制为 `pub(crate)` 或 sealed validation factory；使用按 Decision 区分的构造类型，禁止 Pass 携带 Fatal/Repair Issue。`ValidationIssue` 应补齐 typed code、Repairability 和 Location，而不是自由 `String + bool`。

### P1-06：Character Thought 仍可能成为 World Fact，Validation 也缺少关键阶段

**证据**：当前 Validation 只有 Schema 和引用一致性两个 Validator（[`validation_pipeline.rs:19-56`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/validation/validation_pipeline.rs#L19-L56)）；Consistency 只检查 Character/Memory 引用（[`consistency.rs:9-51`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/validation/validators/consistency.rs#L9-L51)）。任何非空 `proposal.world_change.add_facts` 都会被直接转换为权威 WorldFact（[`validation_pipeline.rs:143-171`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/validation/validation_pipeline.rs#L143-L171)）。

**结果**：如果模型把 Character Thought 中的“猜测/私密认知”原样放入 `add_facts`，现有校验会通过并提交。名为 `character_thought_cannot_become_world_fact` 的测试只构造了一个没有 World Change 的 Proposal，因此没有覆盖真正的越界场景。

**建议**：先补 Modification Permission、Domain Invariant、Knowledge Boundary、Player Control Boundary，再接 Narrative/Character Validator。World Fact Proposal 最好携带可验证的来源或 Evidence ID，不能只凭一段自由文本判断其是否为世界事实。

### P1-07：Context 与 Snapshot 没有完整执行资源上限

**证据**：`set_writer_plan` 和 `set_story_proposal/replace_story_proposal` 不检查集合、字符串或 token 上限；Retrieved/Thought 只检查数量、不检查内容大小（[`turn_context.rs:161-190`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/core/turn_context.rs#L161-L190)、[`turn_context.rs:228-292`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/core/turn_context.rs#L228-L292)）。`SnapshotLimits` 只限制 Recent Turns 和 Memories，不限制 Characters、World Facts 或单项内容（[`turn_data.rs:9-24`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/core/turn_data.rs#L9-L24)）。Retrieval 会先展开并收集全部候选，再排序后截断。

此外，`TurnBudgetLimits::Default` 与 `TurnConfig::Default` 是两套数值且不一致，违反单一配置来源。

**结果**：超大 World JSON、Character 列表、Proposal 或单条 ContextItem 可以绕过 Turn Budget，造成内存、CPU、数据库读取和 Prompt 体积失控。

**建议**：所有阶段输出由 Context 统一验证；SnapshotLimits 补齐各集合及单项 byte/token 上限；Store 查询和 JSON 解码也必须执行上限；Retrieval 使用有界 Top-K，不先构造完整候选数组；删除生产可用的第二套 Budget Default。

### P1-08：LlmGateway 固定事务没有覆盖所有退出路径

**证据**：Gateway 在预算、RPM/TPM 和并发 permit 之后才开始 Turn Trace LLM span（[`gateway.rs:176-211`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/llm/gateway.rs#L176-L211)），因此 pre-cancel、deadline、budget、rate wait 和 queue timeout 不会产生完整 LLM Trace。RateGate 等待只受 Turn deadline 控制，没有使用 `queue_timeout_ms`（[`limiter.rs:88-121`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/llm/limiter.rs#L88-L121)）。

Budget 的 reservation 本身不占用 pending usage；settlement 超限时，Trace 已按 completion 成功写成 `status=ok`，然后才返回 `TokenBudgetExceeded`。

**建议**：使用一个 Gateway Call Transaction/RAII Guard，从入口即创建 span，统一记录所有退出路径；reservation 必须有 reserve/settle/release 语义；Rate quota 和 semaphore 共享同一个有效 queue deadline；settlement 结果必须参与最终 status/error_kind。

### P1-09：`metadata_only` 仍会泄漏 Prompt/Response 内容

**证据**：Planner、Thinker、Generator、Repairer 的 JSON 解析错误把原始模型输出预览拼进错误文本；该错误随后会进入日志、SSE 和 Turn Trace。Engine 还无条件将截断后的 `player_input` 写入根 Trace（[`engine.rs:130-145`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/engine.rs#L130-L145)）。启用 `TraceContent::Content` 时仅截断，没有脱敏步骤。

**结果**：即使配置为默认 `metadata_only`，玩家输入和部分 LLM Response 仍会持久化到 Trace 文件或普通日志，违反 Spec 的内容策略。

**建议**：错误只记录 parse kind、schema path、response hash/length，不包含正文；Turn root trace 的玩家输入也受 TraceContent 策略控制；Content 模式必须先经过统一 Redactor，再截断，并限制为显式开发环境。

### P1-10：生产 shutdown、任务背压和锁规则未闭合

**证据**：`main` 直接 `axum::serve(...).await`，没有 graceful shutdown，也没有调用 TaskManager 或 Coordinator 的 shutdown（[`main.rs:23-36`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise-server/src/main.rs#L23-L36)）。TaskManager 在 semaphore 上无限等待，没有等待队列容量/超时；`shutdown_with_grace` 持有 `tokio::MutexGuard<JoinSet>` 跨 `join_next().await`（[`tasks.rs:36-75`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise-server/src/tasks.rs#L36-L75)）。TaskManager shutdown token 也没有合并进 TurnCancellation。

SSE Sink 还在 `std::sync::Mutex` 锁内执行 channel `try_send`（[`api/sse.rs:63-70`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise-server/src/api/sse.rs#L63-L70)），违反项目 Guardrail。

**建议**：建立服务级 CancellationToken 树；`axum::serve` 接入 signal graceful shutdown；Task Supervisor 单独拥有 JoinSet；admission 使用 `try_acquire` 或有界队列 + timeout；运行中 Turn 获得 shutdown child token；任何 Mutex/RwLock Guard 不得跨 `.await` 或包住 channel send。

### P1-11：StoryReadSnapshot/Baseline/Commit 缺少权威故事状态

**证据**：`StoryReadSnapshot` 目前只有 revision、player ID、可选 World、Characters、Recent Turns 和 Player Memories（[`turn_data.rs:15-73`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/core/turn_data.rs#L15-L73)）。Baseline Builder 将 Story Instructions、Story Config、Active Constraints 设为空，并用最后一段 Story Text 近似 Current Scene、用最近一个 `summary_delta` 近似完整 Summary（[`baseline_ctx_builder.rs:50-78`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/context/baseline_ctx_builder.rs#L50-L78)）。Commit 也没有权威 Scene、Constraint、Summary 状态表的应用逻辑。

**结果**：现有 Writer 虽然流程完整，但没有拿到 Spec 要求的核心故事上下文，摘要也可能被错误解释为完整状态。

**建议**：补齐 Story metadata/state schema、迁移、Snapshot 原子读取和 ChangeSet Commit；`summary_change` 使用明确 `StateChange`/Command，不再把 Turn 的 `summary_delta` 当作当前权威摘要。

### P1-12：Session 与持久 Story 仍是事实上的一对一关系

**证据**：内存 Session 创建时总是生成一个全新 StoryId（[`session/registry.rs:21-32`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise-server/src/session/registry.rs#L21-L32)），没有连接已有 Story 的 API，也没有持久化 Session。

**结果**：删除 Session 或服务重启后，数据库里的 Story 仍存在但 API/UI 无法重新定位它。这与“Session 是临时连接资源、Story 是持久领域对象、两者不构成一对一不变量”的 v3.1 修订不一致。

**建议**：Story 使用独立的持久创建/查询 API；Session 只绑定或切换已有 StoryId，同一 Story 可由多个临时 Session 连接。

### P2-01：OpenAI Compatible Streaming 与错误分类不完整

**证据**：Streaming Provider 不解析 finish reason 和 raw usage，流结束时固定返回 `FinishReason::Stop`；`buf`、`text`、`reasoning` 也没有协议级 byte 上限（[`openai_compat.rs:119-183`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/llm/openai_compat.rs#L119-L183)）。HTTP 429/4xx 通过 `reqwest::error_for_status` 落入 `Transport`，没有转成 `RateLimited/ProviderRejected`。

**建议**：增加独立 Provider Error；按 HTTP status 分类；请求 streaming usage，解析 finish reason/cached tokens；对单行、缓冲区、累计 content/reasoning 设置硬上限。

### P2-02：LLM Usage/Charge 没有形成持久化闭环

**证据**：`CommittedTurnResult` 只保存 calls/input/output/total 聚合（[`turn_contract.rs:229-243`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise/src/core/turn_contract.rs#L229-L243)），缺少 cached input、usage accuracy、provider、model、price version、charge 和单次调用记录；Embedding Output 也不携带 usage/charge。

**建议**：建立受限的 per-call usage ledger，并把聚合 usage/charge 与版本信息放入同一 Turn Commit 事务；幂等重放返回同一账务结果。

### P2-03：Observer 和 Trace 实现仍有交付风险

**证据**：`ValidationCompleted` 只在整个 Runtime 成功并 Commit 后发送一次，Repair/Reject 的每轮 Validation 都不会发送该事件。SSE 队列满时会无差别丢弃事件，包括最终 `Committed/Failed`。文件 Trace Sink 在 async 执行路径同步 open/write/flush，并且没有文件保留或清理策略（[`trace.rs:19-70`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise-server/src/trace.rs#L19-L70)）。

**建议**：Validation Pipeline 每轮发布包含 Decision 的事件；终态事件使用保留槽、独立通道或可恢复结果 API；Trace 交给有界异步 Writer，并配置 rotation/retention。

### P2-04：配置和 CI 尚未完全符合 Spec 的验收方式

**证据**：无效 TOML 会警告后静默回退默认配置（[`server/config.rs:67-89`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/crates/aise-server/src/config.rs#L67-L89)）；TurnConfig 只在首个 Turn 创建 Budget 时校验，CoordinatorConfig 没有完整校验。CI 使用 stable，并执行 `cargo test --workspace`、不执行 Spec 要求的 `--all-features` 和 MSRV 工具链检查（[`.github/workflows/ci.yml:15-27`](https://github.com/falconcali/aise/blob/bf653e5439ff04a563d50fb8be8f8492a6bd8bee/.github/workflows/ci.yml#L15-L27)）。

**建议**：启动时一次性校验完整 Server/Aise/Turn/Coordinator/LLM Config；无效配置直接失败。CI 增加精确的 §16 命令和 `rust-version=1.85` job。

## 5. 测试与验证结论

### 5.1 已确认

- 当前 commit 的远程 CI 成功：Format、Clippy `-D warnings`、Check、Test 全绿。
- Spec 第 16 节的四项旧路径静态检查均无业务代码匹配：旧 Runtime Contract、业务目录 `LlmProvider`、`StoryDraft`、`lock_turn/turn_lock` 已清理。
- 48 个强制测试名称全部存在。
- `git diff --check c14f84e..HEAD` 无空白错误。

### 5.2 当前测试中的弱覆盖

| 测试名 | 实际缺口 |
| --- | --- |
| `llm_trace_closes_on_timeout_and_cancel` | 只验证 ProviderTimeout，没有触发 Cancellation，也没有验证 queue-timeout/pre-dispatch span |
| `character_thought_cannot_become_world_fact` | Proposal 没有尝试把 Thought 写入 `world_change`，因此不能证明边界成立 |
| `snapshot_is_revision_consistent` | 只做顺序的 before/after 读取，没有构造并发 Commit 验证同一 Snapshot 内各表 revision 一致 |
| `bounded_outputs_reject_over_limit_values` | 只检查 Retrieved Item 数量，没有覆盖 Plan、Proposal、Thought 字段、单项长度和 token 总量 |
| `deterministic_failure_cannot_be_overridden_by_narrative_validator` | 当前没有 Narrative Validator，测试只能证明 Schema Reject，不证明 LLM Validator 无法覆盖确定性结果 |
| `response_loss_retry_does_not_call_llm_again` | 只验证 Engine 直接返回值，没有经过 HTTP/SSE，因此未发现重放返回空 SSE 的问题 |

### 5.3 本地执行限制

当前审查环境未安装 Rust 工具链，执行 `cargo fmt --all -- --check` 时得到 `cargo: command not found`，因此本报告没有声称本地运行通过。构建/测试通过结论来自与审查 commit 完全一致的 GitHub Actions run。

## 6. 建议修复顺序

### 第一批：先恢复安全与可恢复性

1. 为 SSE 幂等重放、前置冲突、无效 StoryId 副作用、nested LLM Cancel、nested Store Conflict 增加失败测试。
2. 重构 Engine 为单一 finalizer：统一 Context 终态、错误分类、事件发布和 Trace 关闭。
3. 在任何 Store/permit 操作前验证完整 ExecuteTurnSpec；移除 Turn 入口的隐式 Story 创建。
4. 增加按 idempotency key 查询结果的恢复接口，保证终态 SSE 丢失后仍可恢复。

### 第二批：封闭 Validation 与资源边界

1. 封闭 `ValidatedChangeSet` 构造权限，强制 Decision/Issue 不变量。
2. 补齐 Permission、Domain、Knowledge、Player Control 验证，明确 Thought → Fact 的证据协议。
3. 将 Plan、Snapshot、Retrieved、Thought、Proposal、Validation、Trace 的数量与 byte/token 上限统一收口到 Context/Budget。
4. 拆分 Core/Gateway/Provider/Store/Adapter Error，修复传递依赖方向。

### 第三批：完成 Gateway 与生产生命周期

1. 用统一 LLM Call Transaction 覆盖 reserve、quota、queue、provider、settle、trace 的全部退出路径。
2. 完成 streaming usage、finish reason、HTTP error 分类和协议缓冲上限。
3. 接入 graceful shutdown、服务级 Cancellation、Task Supervisor 和有界 admission queue。
4. 将 Trace 写入改为有界异步 I/O，增加脱敏和 retention。

### 第四批：补齐权威 Story Context

1. 增加 Story Instructions/Config、Current Scene、Summary、Constraints 的权威存储与 Snapshot。
2. Commit 通过明确 ChangeSet/Command 更新这些状态。
3. 解耦 Session 与 Story，支持重新连接持久 Story。
4. 最后再继续高级 Retrieval、Narrative Validator 和 StoryPack 集成。

## 7. 最终判定

当前代码已经从“架构草图”进入了“可运行骨架”，但尚未达到 Spec 的 Definition of Done。**最值得肯定的是主流程结构已经收敛；最需要警惕的是 CI 全绿掩盖了终态交付、错误分类、Validation 信任边界和资源生命周期仍未闭环。**

建议先完成上述前三批修复并补齐对应失败测试，再宣布 Turn Runtime 重构完成；Story Context 的权威状态补齐后，再进入新的业务 Pipeline 增强。

use aise::turn::observability::ContentCapturePolicy;

const DEFAULT_BASE_URL: &str = "https://cloud.langfuse.com";
const DEFAULT_ENVIRONMENT: &str = "development";

#[derive(Debug, Clone, PartialEq)]
pub struct ObservabilityConfig {
    pub enabled: bool,
    pub base_url: String,
    pub public_key: Option<String>,
    pub secret_key: Option<String>,
    pub environment: String,
    pub release: String,
    pub sample_rate: f64,
    pub content_policy: ContentCapturePolicy,
    pub full_content_allowed: bool,
    pub max_field_bytes: usize,
    pub max_observation_bytes: usize,
    pub max_queue_size: usize,
    pub max_export_batch_size: usize,
    pub schedule_delay_ms: u64,
    pub http_timeout_ms: u64,
    pub shutdown_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilityConfigIssue {
    pub field: &'static str,
    pub error_kind: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObservabilityConfigLoad {
    pub config: ObservabilityConfig,
    pub issues: Vec<ObservabilityConfigIssue>,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: DEFAULT_BASE_URL.into(),
            public_key: None,
            secret_key: None,
            environment: DEFAULT_ENVIRONMENT.into(),
            release: env!("CARGO_PKG_VERSION").into(),
            sample_rate: 1.0,
            content_policy: ContentCapturePolicy::MetadataOnly,
            full_content_allowed: false,
            max_field_bytes: 16_384,
            max_observation_bytes: 32_768,
            max_queue_size: 2_048,
            max_export_batch_size: 256,
            schedule_delay_ms: 1_000,
            http_timeout_ms: 3_000,
            shutdown_timeout_ms: 5_000,
        }
    }
}

impl ObservabilityConfig {
    pub fn load_from_env() -> ObservabilityConfigLoad {
        Self::load_with(|name| std::env::var(name).ok())
    }

    pub fn load_with(get: impl Fn(&str) -> Option<String>) -> ObservabilityConfigLoad {
        let mut config = Self::default();
        let mut issues = Vec::new();

        parse_bool(&get, "LANGFUSE_TRACING_ENABLED", &mut config.enabled, &mut issues);
        assign_string(&get, "LANGFUSE_BASE_URL", &mut config.base_url);
        config.public_key = non_empty(get("LANGFUSE_PUBLIC_KEY"));
        config.secret_key = non_empty(get("LANGFUSE_SECRET_KEY"));
        assign_string(&get, "LANGFUSE_TRACING_ENVIRONMENT", &mut config.environment);
        assign_string(&get, "LANGFUSE_RELEASE", &mut config.release);
        parse_number(&get, "LANGFUSE_SAMPLE_RATE", &mut config.sample_rate, &mut issues);
        parse_content_policy(&get, &mut config.content_policy, &mut issues);
        parse_bool(
            &get,
            "AISE_TRACE_FULL_CONTENT_ALLOWED",
            &mut config.full_content_allowed,
            &mut issues,
        );
        parse_number(&get, "AISE_TRACE_MAX_FIELD_BYTES", &mut config.max_field_bytes, &mut issues);
        parse_number(
            &get,
            "AISE_TRACE_MAX_OBSERVATION_BYTES",
            &mut config.max_observation_bytes,
            &mut issues,
        );
        parse_number(&get, "OTEL_BSP_MAX_QUEUE_SIZE", &mut config.max_queue_size, &mut issues);
        parse_number(
            &get,
            "OTEL_BSP_MAX_EXPORT_BATCH_SIZE",
            &mut config.max_export_batch_size,
            &mut issues,
        );
        parse_number(&get, "OTEL_BSP_SCHEDULE_DELAY", &mut config.schedule_delay_ms, &mut issues);
        parse_number(&get, "AISE_TRACE_HTTP_TIMEOUT_MS", &mut config.http_timeout_ms, &mut issues);
        parse_number(
            &get,
            "AISE_TRACE_SHUTDOWN_TIMEOUT_MS",
            &mut config.shutdown_timeout_ms,
            &mut issues,
        );

        validate(&mut config, &mut issues);
        ObservabilityConfigLoad { config, issues }
    }

    pub fn endpoint(&self) -> Option<String> {
        if !self.enabled {
            return None;
        }
        Some(format!(
            "{}/api/public/otel/v1/traces",
            self.base_url.trim().trim_end_matches('/')
        ))
    }

    pub fn endpoint_host(&self) -> Option<String> {
        reqwest::Url::parse(self.base_url.trim())
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
    }
}

fn assign_string(get: &impl Fn(&str) -> Option<String>, name: &str, target: &mut String) {
    if let Some(value) = get(name) {
        *target = value;
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn parse_bool(
    get: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    target: &mut bool,
    issues: &mut Vec<ObservabilityConfigIssue>,
) {
    if let Some(value) = get(name) {
        match value.parse::<bool>() {
            Ok(value) => *target = value,
            Err(_) => issues.push(issue(name, "invalid_boolean")),
        }
    }
}

fn parse_number<T>(
    get: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    target: &mut T,
    issues: &mut Vec<ObservabilityConfigIssue>,
) where
    T: std::str::FromStr,
{
    if let Some(value) = get(name) {
        match value.parse::<T>() {
            Ok(value) => *target = value,
            Err(_) => issues.push(issue(name, "invalid_number")),
        }
    }
}

fn parse_content_policy(
    get: &impl Fn(&str) -> Option<String>,
    target: &mut ContentCapturePolicy,
    issues: &mut Vec<ObservabilityConfigIssue>,
) {
    if let Some(value) = get("AISE_TRACE_CONTENT_POLICY") {
        match value.as_str() {
            "metadata_only" => *target = ContentCapturePolicy::MetadataOnly,
            "redacted_content" => *target = ContentCapturePolicy::RedactedContent,
            "full_content" => *target = ContentCapturePolicy::FullContent,
            _ => issues.push(issue("AISE_TRACE_CONTENT_POLICY", "invalid_content_policy")),
        }
    }
}

fn validate(config: &mut ObservabilityConfig, issues: &mut Vec<ObservabilityConfigIssue>) {
    let enabled_requested = config.enabled;

    if !(0.0..=1.0).contains(&config.sample_rate) || !config.sample_rate.is_finite() {
        issues.push(issue("LANGFUSE_SAMPLE_RATE", "out_of_range"));
    }
    if config.max_field_bytes == 0 {
        issues.push(issue("AISE_TRACE_MAX_FIELD_BYTES", "must_be_positive"));
    }
    if config.max_observation_bytes == 0 {
        issues.push(issue("AISE_TRACE_MAX_OBSERVATION_BYTES", "must_be_positive"));
    }
    if config.max_queue_size == 0 {
        issues.push(issue("OTEL_BSP_MAX_QUEUE_SIZE", "must_be_positive"));
    }
    if config.max_export_batch_size == 0 || config.max_export_batch_size > config.max_queue_size {
        issues.push(issue("OTEL_BSP_MAX_EXPORT_BATCH_SIZE", "out_of_range"));
    }
    if config.schedule_delay_ms == 0 {
        issues.push(issue("OTEL_BSP_SCHEDULE_DELAY", "must_be_positive"));
    }
    if config.http_timeout_ms == 0 {
        issues.push(issue("AISE_TRACE_HTTP_TIMEOUT_MS", "must_be_positive"));
    }
    if config.shutdown_timeout_ms == 0 || config.http_timeout_ms >= config.shutdown_timeout_ms {
        issues.push(issue("AISE_TRACE_SHUTDOWN_TIMEOUT_MS", "insufficient_shutdown_budget"));
    }
    if config.environment.trim().is_empty()
        || !config
            .environment
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '.' | '_' | '-'))
    {
        issues.push(issue("LANGFUSE_TRACING_ENVIRONMENT", "invalid_environment"));
    }
    if config.release.trim().is_empty() {
        issues.push(issue("LANGFUSE_RELEASE", "empty_release"));
    }
    if matches!(config.content_policy, ContentCapturePolicy::FullContent)
        && (config.environment != "development" || !config.full_content_allowed)
    {
        config.content_policy = ContentCapturePolicy::MetadataOnly;
        issues.push(issue("AISE_TRACE_CONTENT_POLICY", "full_content_not_allowed"));
    }

    if enabled_requested {
        if config.public_key.is_none() {
            issues.push(issue("LANGFUSE_PUBLIC_KEY", "missing_credential"));
        }
        if config.secret_key.is_none() {
            issues.push(issue("LANGFUSE_SECRET_KEY", "missing_credential"));
        }
        match reqwest::Url::parse(config.base_url.trim()) {
            Ok(url)
                if matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none() => {}
            _ => issues.push(issue("LANGFUSE_BASE_URL", "invalid_url")),
        }
    }

    if !issues.is_empty() {
        config.enabled = false;
    }
}

const fn issue(field: &'static str, error_kind: &'static str) -> ObservabilityConfigIssue {
    ObservabilityConfigIssue { field, error_kind }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;

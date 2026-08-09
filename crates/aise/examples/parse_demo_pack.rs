use aise::config::AssetLimitsConfig;
use aise::domain::asset::ids::{PlayerId, StoryRoleKey};
use aise::persistence::asset_store::AssetStore;
use aise::persistence::sqlite_asset_store::SqliteAssetStore;
use aise::persistence::{SqliteStore, Store};
use aise::story::instance_factory::{CreateStoryInstanceSpec, StoryInstanceFactory, StoryInstantiationLimits};
use aise::story::pack_service::{AssetInput, NativeAssetImporter, PackService};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::main]
async fn main() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/demo_pack.json");
    let json = std::fs::read_to_string(path).expect("read");
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let db = std::env::temp_dir().join(format!("aise_demo_import_{now}.db"));
    let db_url = db.to_string_lossy().into_owned();
    let store: Arc<dyn Store> = SqliteStore::connect(&db_url).await.unwrap();
    let asset_store: Arc<dyn AssetStore> = SqliteAssetStore::connect(&db_url).await.unwrap();
    let importer = NativeAssetImporter::new(AssetLimitsConfig::default());
    let pack_service = PackService::new(importer, asset_store.clone());
    match pack_service.import(AssetInput::Json(json.as_bytes())).await {
        Ok(info) => {
            println!("IMPORT OK pack_id={} key={}", info.pack_id, info.pack_key);
            let factory = StoryInstanceFactory::new(
                asset_store,
                store,
                StoryInstantiationLimits {
                    max_roles: 32,
                    max_facts: 512,
                    max_rumors: 256,
                    max_memories: 32,
                    max_relationships: 32,
                },
            );
            let spec = CreateStoryInstanceSpec {
                pack_id: info.pack_id.clone(),
                player_id: PlayerId::from("player-1"),
                player_role_key: StoryRoleKey::from("protagonist"),
                player_character: None,
                created_at_ms: 1,
            };
            match factory.create(spec).await {
                Ok(story) => println!("INSTANCE OK story_id={}", story.story_id),
                Err(e) => println!("INSTANCE ERR: {e:?}"),
            }
        }
        Err(e) => println!("IMPORT ERR: {e:?}"),
    }
}

use crate::REDIS_URL;
use redis::{aio::ConnectionManager, Client};

pub async fn init_redis() -> ConnectionManager {
    let redis_url = &*REDIS_URL.clone();

    ConnectionManager::new(Client::open(redis_url).expect("A valid redis connection"))
        .await
        .expect("A valid connection manager")
}

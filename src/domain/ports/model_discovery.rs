use async_trait::async_trait;

#[async_trait]
pub trait ModelDiscoveryPort: Send + Sync {
    async fn fetch_available_models(&self) -> Vec<(String, String)>;
}

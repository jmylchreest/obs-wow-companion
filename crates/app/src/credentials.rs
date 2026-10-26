//! Passwords live only in the OS credential store, never in app settings.
use anyhow::{Result, anyhow};
use keyring::Entry;

fn entry(scope: &str, port: u16) -> Result<Entry> {
    Entry::new(
        "org.obs-wow-companion.obs",
        &format!("{scope}:127.0.0.1:{port}"),
    )
    .map_err(|_| anyhow!("Could not access the system password store."))
}
pub async fn read(scope: String, port: u16) -> Result<Option<String>> {
    tokio::task::spawn_blocking(move || match entry(&scope, port)?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(anyhow!("Unlock your system password store, or enter the OBS password to connect for this session.")),
    }).await.map_err(|_| anyhow!("Password-store task failed."))?
}
pub async fn save(scope: String, port: u16, password: String) -> Result<()> {
    tokio::task::spawn_blocking(move || entry(&scope, port)?.set_password(&password)
        .map_err(|_| anyhow!("Connected, but the password could not be saved. Unlock or enable your system password store, then enter the password and check again.")))
        .await.map_err(|_| anyhow!("Password-store task failed."))?
}
pub async fn forget(scope: String, port: u16) -> Result<()> {
    tokio::task::spawn_blocking(move || match entry(&scope, port)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(anyhow!(
            "Could not remove the saved password. Unlock your system password store and try again."
        )),
    })
    .await
    .map_err(|_| anyhow!("Password-store task failed."))?
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    #[ignore = "requires an isolated OS password store; run scripts/credential-smoke.py"]
    async fn system_store_round_trip() {
        assert_eq!(
            std::env::var("OWC_TEST_PRIVATE_KEYRING").as_deref(),
            Ok("1")
        );
        let scope = uuid::Uuid::new_v4().to_string();
        assert!(
            super::read(scope.clone(), 4455)
                .await
                .expect("read empty")
                .is_none()
        );
        super::save(scope.clone(), 4455, "private-fixture-password".into())
            .await
            .expect("save");
        assert_eq!(
            super::read(scope.clone(), 4455)
                .await
                .expect("read")
                .as_deref(),
            Some("private-fixture-password")
        );
        assert!(
            super::read(scope.clone(), 4456)
                .await
                .expect("other port")
                .is_none()
        );
        super::forget(scope.clone(), 4455).await.expect("forget");
        assert!(
            super::read(scope, 4455)
                .await
                .expect("read deleted")
                .is_none()
        );
    }
}

use rusqlite::{Connection, OptionalExtension};

pub const OPENAI_ACCOUNT: &str = "openai_api_key";
pub const SERVICE_NAME: &str = "ai.cppinsight.desktop";

pub trait SecretBackend: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct OsSecretBackend;

impl OsSecretBackend {
    fn entry(account: &str) -> Result<keyring::v1::Entry, String> {
        keyring::v1::Entry::new(SERVICE_NAME, account)
            .map_err(|error| format!("failed to open operating-system credential store: {error}"))
    }
}

impl SecretBackend for OsSecretBackend {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match Self::entry(account)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::v1::Error::NoEntry) => Ok(None),
            Err(error) => Err(format!(
                "failed to read operating-system credential: {error}"
            )),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        Self::entry(account)?
            .set_password(value)
            .map_err(|error| format!("failed to save operating-system credential: {error}"))
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::v1::Error::NoEntry) => Ok(()),
            Err(error) => Err(format!(
                "failed to delete operating-system credential: {error}"
            )),
        }
    }
}

pub fn migrate_legacy_api_key(
    connection: &Connection,
    backend: &dyn SecretBackend,
) -> Result<bool, String> {
    let legacy: Option<String> = connection
        .query_row(
            "SELECT value FROM settings WHERE key = 'openai_api_key'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("failed to read legacy API key: {error}"))?;
    let Some(secret) = legacy.map(|value| value.trim().to_string()) else {
        return Ok(false);
    };
    if secret.is_empty() {
        return Ok(false);
    }

    backend.set(OPENAI_ACCOUNT, &secret)?;
    connection
        .execute("DELETE FROM settings WHERE key = 'openai_api_key'", [])
        .map_err(|error| format!("failed to remove legacy API key: {error}"))?;
    Ok(true)
}

pub fn update_api_key(
    backend: &dyn SecretBackend,
    new_value: Option<&str>,
    clear: bool,
) -> Result<(), String> {
    if clear {
        return backend.delete(OPENAI_ACCOUNT);
    }
    if let Some(value) = new_value.map(str::trim).filter(|value| !value.is_empty()) {
        backend.set(OPENAI_ACCOUNT, value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, OptionalExtension};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemorySecretBackend {
        values: Mutex<HashMap<String, String>>,
    }

    impl SecretBackend for MemorySecretBackend {
        fn get(&self, account: &str) -> Result<Option<String>, String> {
            Ok(self.values.lock().unwrap().get(account).cloned())
        }

        fn set(&self, account: &str, value: &str) -> Result<(), String> {
            self.values
                .lock()
                .unwrap()
                .insert(account.to_string(), value.to_string());
            Ok(())
        }

        fn delete(&self, account: &str) -> Result<(), String> {
            self.values.lock().unwrap().remove(account);
            Ok(())
        }
    }

    struct FailingSecretBackend;

    impl SecretBackend for FailingSecretBackend {
        fn get(&self, _account: &str) -> Result<Option<String>, String> {
            Ok(None)
        }

        fn set(&self, _account: &str, _value: &str) -> Result<(), String> {
            Err("credential store unavailable".to_string())
        }

        fn delete(&self, _account: &str) -> Result<(), String> {
            Ok(())
        }
    }

    fn legacy_settings_db(value: &str) -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        connection
            .execute(
                "INSERT INTO settings(key, value) VALUES('openai_api_key', ?1)",
                params![value],
            )
            .unwrap();
        connection
    }

    fn legacy_key(connection: &Connection) -> Option<String> {
        connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'openai_api_key'",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap()
    }

    #[test]
    fn successful_migration_removes_plaintext_after_secret_is_saved() {
        let connection = legacy_settings_db("sk-legacy");
        let backend = MemorySecretBackend::default();

        assert!(migrate_legacy_api_key(&connection, &backend).unwrap());
        assert_eq!(
            backend.get(OPENAI_ACCOUNT).unwrap().as_deref(),
            Some("sk-legacy")
        );
        assert_eq!(legacy_key(&connection), None);
    }

    #[test]
    fn failed_secret_write_preserves_plaintext_for_retry() {
        let connection = legacy_settings_db("sk-preserve");

        assert!(migrate_legacy_api_key(&connection, &FailingSecretBackend).is_err());
        assert_eq!(legacy_key(&connection).as_deref(), Some("sk-preserve"));
    }

    #[test]
    fn migration_without_legacy_key_is_a_noop() {
        let connection = legacy_settings_db("");
        connection
            .execute("DELETE FROM settings WHERE key = 'openai_api_key'", [])
            .unwrap();
        let backend = MemorySecretBackend::default();

        assert!(!migrate_legacy_api_key(&connection, &backend).unwrap());
        assert_eq!(backend.get(OPENAI_ACCOUNT).unwrap(), None);
    }

    #[test]
    fn empty_update_preserves_existing_secret() {
        let backend = MemorySecretBackend::default();
        backend.set(OPENAI_ACCOUNT, "sk-existing").unwrap();

        update_api_key(&backend, None, false).unwrap();

        assert_eq!(
            backend.get(OPENAI_ACCOUNT).unwrap().as_deref(),
            Some("sk-existing")
        );
    }

    #[test]
    fn explicit_clear_deletes_existing_secret() {
        let backend = MemorySecretBackend::default();
        backend.set(OPENAI_ACCOUNT, "sk-existing").unwrap();

        update_api_key(&backend, None, true).unwrap();

        assert_eq!(backend.get(OPENAI_ACCOUNT).unwrap(), None);
    }

    #[test]
    fn nonempty_update_replaces_existing_secret() {
        let backend = MemorySecretBackend::default();
        backend.set(OPENAI_ACCOUNT, "sk-old").unwrap();

        update_api_key(&backend, Some("  sk-new  "), false).unwrap();

        assert_eq!(
            backend.get(OPENAI_ACCOUNT).unwrap().as_deref(),
            Some("sk-new")
        );
    }
}

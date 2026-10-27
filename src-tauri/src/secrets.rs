//! API keys live in the OS keychain (Keychain, Credential Manager, Secret Service).
//! They are never written to settings.json and never sent to the webview.

use contextlens_core::Provider;

const SERVICE: &str = "dev.contextlens.app";

fn entry(p: Provider) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, p.id()).map_err(|e| e.to_string())
}

pub fn get(p: Provider) -> Option<String> {
    entry(p).ok()?.get_password().ok().filter(|k| !k.is_empty())
}

pub fn set(p: Provider, key: &str) -> Result<(), String> {
    entry(p)?.set_password(key.trim()).map_err(|e| e.to_string())
}

pub fn delete(p: Provider) -> Result<(), String> {
    match entry(p)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

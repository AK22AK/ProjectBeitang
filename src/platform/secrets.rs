const SERVICE_NAME: &str = "Robinne";

pub fn load_secret(account: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE_NAME, account)
        .map_err(|err| format!("初始化系统密钥存储失败: {err}"))?;

    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(format!("读取系统密钥失败: {err}")),
    }
}

pub fn save_secret(account: &str, secret: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, account)
        .map_err(|err| format!("初始化系统密钥存储失败: {err}"))?;
    entry
        .set_password(secret)
        .map_err(|err| format!("保存系统密钥失败: {err}"))
}

pub fn delete_secret(account: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, account)
        .map_err(|err| format!("初始化系统密钥存储失败: {err}"))?;
    match entry.delete_credential() {
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(format!("删除系统密钥失败: {err}")),
    }
}

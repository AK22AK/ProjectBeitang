pub fn load_secret(env_var: &str) -> Result<Option<String>, String> {
    Ok(std::env::var(env_var)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

pub fn save_secret(env_var: &str, secret: &str) -> Result<(), String> {
    let secret = secret.trim();
    if secret.is_empty() {
        return Err("API Key 不能为空".to_string());
    }
    std::env::set_var(env_var, secret);
    Ok(())
}

pub fn delete_secret(env_var: &str) -> Result<(), String> {
    std::env::remove_var(env_var);
    Ok(())
}

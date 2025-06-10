pub fn verify_json(file: &[u8]) -> anyhow::Result<()> {
    serde_json::from_slice::<serde_json::Value>(file)?;
    Ok(())
}

use libuvmanager::PythonEnvironment;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let environment = PythonEnvironment::new(".venv")?;
    environment.create(None).await?;
    environment.install(&["packaging"]).await?;

    let output = environment.run_code("import packaging; print(packaging.__version__)")?;
    if !output.status.success() {
        return Err(format!("Python exited with {}", output.status).into());
    }
    println!("{}", String::from_utf8_lossy(&output.stdout).trim());
    Ok(())
}

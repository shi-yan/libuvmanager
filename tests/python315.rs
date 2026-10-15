use libuvmanager::PythonEnvironment;

#[test]
#[ignore = "downloads CPython 3.15 if it is not cached"]
fn python315_and_local_wheel_lifecycle() {
    let root = std::env::temp_dir().join(format!(
        "libuvmanager-smoke-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let env = PythonEnvironment::new(root.join("venv")).unwrap();
    let wheel = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/libuvmanager_smoke-1.0-py3-none-any.whl");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        env.create(Some("3.15.0")).await.unwrap();
        let version = env.run_code("import sys; assert sys.version_info[:3] == (3, 15, 0)").unwrap();
        assert!(version.status.success(), "{:?}", version);
        env.install(&[wheel.to_str().unwrap()]).await.unwrap();
        let imported = env.run_code("import libuvmanager_smoke; assert libuvmanager_smoke.VALUE == 'local-wheel'").unwrap();
        assert!(imported.status.success(), "{:?}", imported);
        let requirements = root.join("requirements.txt");
        std::fs::write(&requirements, format!("{}\n", wheel.display())).unwrap();
        env.sync_requirements(&requirements).await.unwrap();
        env.uninstall(&["libuvmanager-smoke"]).await.unwrap();
        let removed = env.run_code("import importlib.util; assert importlib.util.find_spec('libuvmanager_smoke') is None").unwrap();
        assert!(removed.status.success(), "{:?}", removed);
    });
    std::fs::remove_dir_all(root).unwrap();
}

use std::{fs, io, process::Command};

const VIEWER_HTML: &str = include_str!("../viewer/index.html");

pub fn open() -> Result<(), String> {
    let dir = std::env::temp_dir().join("onmkFlare");
    fs::create_dir_all(&dir).map_err(error)?;
    let path = dir.join("FlareStudio.html");
    fs::write(&path, VIEWER_HTML).map_err(error)?;
    #[cfg(target_os = "windows")]
    Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map_err(error)?;
    #[cfg(target_os = "macos")]
    Command::new("open").arg(&path).spawn().map_err(error)?;
    Ok(())
}

fn error(e: io::Error) -> String {
    e.to_string()
}

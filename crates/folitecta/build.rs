//! The Windows program gets its icon, its version information and a
//! manifest (version 6 of the common controls, for the dialogs of the
//! system). Nothing is done for the other systems.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon-256.png");
    println!("cargo:rerun-if-changed=assets/icon-64.png");
    // The resource compiler of the Windows tools, on a Windows computer.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows::resources();
    }
}

/// An icon file of PNG pictures, as Windows Vista and later read them: a
/// header, one entry for each picture, then the pictures.
#[cfg_attr(not(windows), allow(dead_code))]
fn icon_file(pngs: &[&[u8]]) -> Vec<u8> {
    let mut out = vec![0, 0, 1, 0];
    out.extend_from_slice(&(pngs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pngs.len();
    for png in pngs {
        // The size is in the IHDR chunk, right after the PNG signature.
        let size = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
        // 0 stands for 256 pixels.
        let side = |v: u32| if v >= 256 { 0 } else { v as u8 };
        out.push(side(size(16)));
        out.push(side(size(20)));
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += png.len();
    }
    for png in pngs {
        out.extend_from_slice(png);
    }
    out
}

#[cfg(windows)]
mod windows {
    const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
        processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <!-- Windows 10 and 11 -->
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>
"#;

    pub fn resources() {
        let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
        let big = std::fs::read("assets/icon-256.png").expect("assets/icon-256.png");
        let small = std::fs::read("assets/icon-64.png").expect("assets/icon-64.png");
        let icon = out.join("folitecta.ico");
        std::fs::write(&icon, super::icon_file(&[&big, &small])).expect("the icon file");
        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon.to_str().expect("a path in UTF-8"))
            .set("ProductName", "Folitecta")
            .set("FileDescription", "Folitecta, a free PDF editor")
            .set("LegalCopyright", "MIT License, Copyright (c) 2026 KvasFan")
            .set_manifest(MANIFEST);
        res.compile().expect("the resources of the Windows program");
    }
}

use std::collections::HashMap;
use std::env;
use std::env::current_dir;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use vergen::BuildBuilder;
use vergen::CargoBuilder;
use vergen::Emitter;

fn main() {
    let build = BuildBuilder::default()
        .build_timestamp(true)
        .build()
        .unwrap();
    let cargo = CargoBuilder::default().target_triple(true).build().unwrap();
    Emitter::default()
        .add_instructions(&build)
        .unwrap()
        .add_instructions(&cargo)
        .unwrap()
        .emit()
        .unwrap();

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let profile = env::var("PROFILE").unwrap();

    let (lib_prefix, lib_extension) = match target_os.as_str() {
        "windows" => ("", "dll"),
        "macos" => ("lib", "dylib"),
        // "linux" => ("lib", "so"), Untested Now
        "android" => ("lib", "so"),
        _ => panic!("Unsupported target OS: {}", target_os),
    };

    let lib_name = format!("{}{}.{}", lib_prefix, "rust", lib_extension);
    let output_dir = env::var("OUT_DIR").unwrap();
    let output_dir = Path::new(&output_dir);
    let godot_project_dir = find_godot_project_dir().unwrap();
    println!("godot_project_dir: {:?}", godot_project_dir);
    let relative_dir = output_dir.strip_prefix(&godot_project_dir).unwrap();
    let mut components: Vec<_> = relative_dir.components().collect();
    if let Some(index) = components.iter().position(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "debug" || s == "release"
    }) {
        // 保留到debug/release目录
        components.truncate(index + 1);
    }
    let lib_dir = components
        .iter()
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned();
    let lib_path = format!("res://{}/{}", lib_dir.replace("\\", "/"), lib_name);
    let godot_triplet = match (target_os.as_str(), target_arch.as_str()) {
        ("macos", "aarch64") => format!("{}.{}", "macos", profile),
        ("android", "aarch64") => format!("android.{}.arm64", profile),
        _ => format!("{}.{}.{}", target_os, profile, target_arch),
    };

    let manifest_dir_owned = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_dir = Path::new(&manifest_dir_owned);
    let gdextension_file_path = manifest_dir.join("rust.gdextension");
    if is_needing_updation(&gdextension_file_path, &godot_triplet, &lib_path) {
        println!("Generating rust.gdextension for {}", godot_triplet);
        generate_gdextension_file(&gdextension_file_path, &godot_triplet, &lib_path);
    } else {
        println!("rust.gdextension is up to date for {}", godot_triplet);
    }
    println!("cargo::rerun-if-changed=Cargo.toml");
}

fn find_godot_project_dir() -> Option<PathBuf> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").ok()?;
    let mut res_dir = PathBuf::from(&manifest_dir);
    loop {
        if res_dir.join("project.godot").exists() {
            return Some(res_dir);
        }
        if !res_dir.pop() {
            return None;
        }
    }
}

fn is_needing_updation(
    gdext_file_path: &Path,
    current_triplet: &str,
    expected_lib_path: &str,
) -> bool {
    if !gdext_file_path.exists() {
        return true;
    }
    if let Ok(content) = fs::read_to_string(gdext_file_path) {
        let parsed_content = parse_gdext_lib(content.as_str());
        match parsed_content.get(current_triplet) {
            Some(lib_path) => lib_path != expected_lib_path,
            None => true,
        }
    } else {
        return true;
    }
}

fn parse_gdext_lib(file_content: &str) -> HashMap<String, String> {
    let mut res = HashMap::new();
    let mut is_in_libraries_section = false;

    for line in file_content.lines() {
        let line = line.trim();
        if is_in_libraries_section || line == "[libraries]" {
            is_in_libraries_section = true;
            continue;
        }
        if is_in_libraries_section {
            let assign_pos = line
                .find('=')
                .expect("Failed to find = in libraries paring");
            let key = line[..assign_pos].trim();
            let value = line[assign_pos + 1..].trim().trim_matches('"');
            res.insert(key.to_string(), value.to_string());
        }
    }
    res
}

fn generate_gdextension_file(gdext_file_path: &Path, current_triplet: &str, lib_path: &str) {
    // 读取现有文件内容或创建新的
    let content = if gdext_file_path.exists() {
        fs::read_to_string(gdext_file_path).unwrap_or_default()
    } else {
        String::new()
    };

    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    // 处理 configuration 部分
    ensure_configuration_section(&mut lines);

    // 处理 libraries 部分
    ensure_libraries_section(&mut lines, current_triplet, lib_path);

    // 写入文件
    fs::write(gdext_file_path, lines.join("\n")).expect("Failed to write rust.gdextension file");
}

fn ensure_configuration_section(lines: &mut Vec<String>) {
    let config_section_header = "[configuration]";
    let required_settings = vec![
        "entry_symbol=\"gdext_rust_init\"",
        "compatibility_minimum=\"4.5\"",
        "reloadable=\"true\"",
    ];

    // 查找或创建 configuration 部分
    let config_section_start = find_section_start(lines, "configuration");

    if config_section_start.is_none() {
        // 如果不存在，在文件末尾添加
        if !lines.is_empty() && !lines.last().unwrap().is_empty() {
            lines.push(String::new());
        }
        lines.push(config_section_header.to_string());
        for setting in &required_settings {
            lines.push(setting.to_string());
        }
        return;
    }

    let start_index = config_section_start.unwrap();

    // 更新或添加必要的设置
    for setting in required_settings {
        let (key, _) = parse_key_value(setting).unwrap();
        if let Some((index, _)) = find_setting_in_section(lines, start_index, key) {
            // 更新现有设置
            lines[index] = setting.to_string();
        } else {
            // 在 section 内插入新设置
            let insert_pos = find_section_end(lines, start_index);
            lines.insert(insert_pos, setting.to_string());
        }
    }
}

fn ensure_libraries_section(lines: &mut Vec<String>, current_triplet: &str, lib_path: &str) {
    let libs_section_header = "[libraries]";
    let new_entry = format!("{}=\"{}\"", current_triplet, lib_path.trim_matches('"'));

    // 查找或创建 libraries 部分
    let libs_section_start = find_section_start(lines, "libraries");

    if libs_section_start.is_none() {
        // 如果不存在，在文件末尾添加
        if !lines.is_empty() && !lines.last().unwrap().is_empty() {
            lines.push(String::new());
        }
        lines.push(libs_section_header.to_string());
        lines.push(new_entry);
        return;
    }

    let start_index = libs_section_start.unwrap();

    // 更新或添加 triplet 设置
    if let Some((index, _)) = find_setting_in_section(lines, start_index, current_triplet) {
        // 更新现有设置
        lines[index] = new_entry;
    } else {
        // 在 section 内插入新设置
        let insert_pos = find_section_end(lines, start_index);
        lines.insert(insert_pos, new_entry);
    }
}

fn find_section_start(lines: &[String], section_name: &str) -> Option<usize> {
    let section_header = format!("[{}]", section_name);
    lines.iter().position(|line| line.trim() == section_header)
}

fn find_section_end(lines: &[String], start_index: usize) -> usize {
    for i in (start_index + 1)..lines.len() {
        if lines[i].trim().starts_with('[') && lines[i].trim().ends_with(']') {
            return i; // 下一个 section 的开始
        }
    }
    lines.len() // 文件结束
}

fn find_setting_in_section(
    lines: &[String],
    section_start: usize,
    key: &str,
) -> Option<(usize, String)> {
    let section_end = find_section_end(lines, section_start);

    for i in (section_start + 1)..section_end {
        let line = lines[i].trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue; // 跳过空行和注释
        }

        if let Some((k, v)) = parse_key_value(line) {
            if k == key {
                return Some((i, v.to_string()));
            }
        }
    }
    None
}

fn parse_key_value(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    let parts: Vec<&str> = line.splitn(2, '=').collect();
    if parts.len() == 2 {
        let key = parts[0].trim();
        let value = parts[1].trim();
        Some((key, value))
    } else {
        None
    }
}

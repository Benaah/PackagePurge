use std::fs;
use std::path::Path;

pub type DepList = Vec<(String, String)>; // (name, version)

pub fn parse_npm_package_lock(path: &Path) -> DepList {
	let mut deps_list: DepList = Vec::new();
	let text = match fs::read_to_string(path) { Ok(t) => t, Err(_) => return deps_list };
	let json: serde_json::Value = match serde_json::from_str(&text) { Ok(v) => v, Err(_) => return deps_list };
	
	fn walk(node: &serde_json::Value, list: &mut DepList) {
		if let Some(deps) = node.get("dependencies").and_then(|d| d.as_object()) {
			for (name, dep_node) in deps {
				if let Some(ver) = dep_node.get("version").and_then(|v| v.as_str()) {
					list.push((name.clone(), ver.to_string()));
				}
				walk(dep_node, list);
			}
		}
        // Handle 'packages' in lockfile v2/v3
        if let Some(packages) = node.get("packages").and_then(|d| d.as_object()) {
            for (key, pkg_node) in packages {
                if key.is_empty() { continue; } // Root
                
                // Key is path like "node_modules/pkg" or "node_modules/a/node_modules/b"
                // We want the package name, which is after the last "node_modules/"
                let name = if let Some(idx) = key.rfind("node_modules/") {
                    key[idx + "node_modules/".len()..].to_string()
                } else {
                    key.clone()
                };

                if let Some(ver) = pkg_node.get("version").and_then(|v| v.as_str()) {
                    list.push((name, ver.to_string()));
                }
            }
        }
	}
    
	walk(&json, &mut deps_list);
	deps_list
}

pub fn parse_yarn_lock(path: &Path) -> DepList {
	let mut list: DepList = Vec::new();
	let text = match fs::read_to_string(path) { Ok(t) => t, Err(_) => return list };
	
	let mut current_name: Option<String> = None;
    
	for line in text.lines() {
		let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        
		if !line.starts_with(' ') {
            // Start of a block: "pkg@ver, pkg@ver:"
            let parts: Vec<&str> = trimmed.trim_end_matches(':').split(',').collect();
            if let Some(first) = parts.first() {
                // Extract name from "name@range"
                // This is heuristic; yarn lock keys are complex.
                // Simpler: wait for "version" line.
                // But we need the name.
                // Pattern: name@^1.2.3
                // Last '@' separates name and version range, but scoped packages start with @.
                let s = first.trim().trim_matches('"');
                if let Some(idx) = s.rfind('@') {
                    if idx > 0 {
                        current_name = Some(s[..idx].to_string());
                    } else {
                        current_name = None;
                    }
                }
            }
		} else if let Some(name) = &current_name {
            if trimmed.starts_with("version") {
                // version "1.2.3"
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let ver = parts[1].trim_matches('"');
                    list.push((name.clone(), ver.to_string()));
                    current_name = None; // Reset so we don't duplicate
                }
            }
        }
	}
	list
}

pub fn parse_pnpm_lock(path: &Path) -> DepList {
	let mut list: DepList = Vec::new();
	let text = match fs::read_to_string(path) { Ok(t) => t, Err(_) => return list };
	
	for line in text.lines() {
		let l = line.trim();
        // /name/version:
		if l.starts_with('/') && l.ends_with(':') {
            let content = l.trim_end_matches(':');
            // content is like /@babel/core/7.2.0
            // extract name and version.
            // Split by '/'
            let parts: Vec<&str> = content.split('/').collect();
            // parts[0] is empty
            // if scoped: "", "@scope", "pkg", "ver" -> len 4
            // if unscoped: "", "pkg", "ver" -> len 3
            if parts.len() >= 3 {
                let ver = parts.last().unwrap().to_string();
                let name = parts[1..parts.len()-1].join("/");
                list.push((name, ver));
            }
		}
	}
	list
}

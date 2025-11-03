use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub type EdgeList = Vec<(String, String)>; // parent -> dependency

pub fn parse_npm_package_lock(path: &Path) -> EdgeList {
	let mut edges: EdgeList = Vec::new();
	let text = match fs::read_to_string(path) { Ok(t) => t, Err(_) => return edges };
	let json: serde_json::Value = match serde_json::from_str(&text) { Ok(v) => v, Err(_) => return edges };
	fn walk(name: &str, node: &serde_json::Value, edges: &mut EdgeList) {
		if let Some(deps) = node.get("dependencies").and_then(|d| d.as_object()) {
			for (dep_name, dep_node) in deps {
				let parent = name.to_string();
				let child = dep_name.to_string();
				edges.push((parent.clone(), child.clone()));
				walk(dep_name, dep_node, edges);
			}
		}
	}
	if let Some(deps) = json.get("dependencies").and_then(|d| d.as_object()) {
		for (name, node) in deps { walk(name, node, &mut edges); }
	}
	edges
}

pub fn parse_yarn_lock(path: &Path) -> EdgeList {
	let mut edges: EdgeList = Vec::new();
	let text = match fs::read_to_string(path) { Ok(t) => t, Err(_) => return edges };
	// Minimal yarn.lock parser: capture blocks and their dependencies
	let mut current: Option<String> = None;
	for line in text.lines() {
		let trimmed = line.trim_end();
		if trimmed.ends_with(":") && !trimmed.starts_with(' ') {
			// e.g. "react@^18.2.0":
			let key = trimmed.trim_end_matches(':').trim().trim_matches('"').to_string();
			current = Some(key);
			continue;
		}
		if trimmed.starts_with("dependencies:") {
			// subsequent indented lines: depName "version"
			continue;
		}
		if let Some(cur) = &current {
			if trimmed.starts_with(char::is_alphabetic) && trimmed.contains(' ') {
				let mut parts = trimmed.split_whitespace();
				if let Some(dep) = parts.next() {
					edges.push((cur.clone(), dep.to_string()));
				}
			}
		}
	}
	edges
}

pub fn parse_pnpm_lock(path: &Path) -> EdgeList {
	let mut edges: EdgeList = Vec::new();
	let text = match fs::read_to_string(path) { Ok(t) => t, Err(_) => return edges };
	// Very lightweight YAML-ish scan: find lines under 'packages:' where dependencies are listed
	let mut in_packages = false;
	let mut current: Option<String> = None;
	for line in text.lines() {
		let l = line.trim_end();
		if l.starts_with("packages:") { in_packages = true; continue; }
		if !in_packages { continue; }
		if l.starts_with('"') || l.starts_with('/') || l.ends_with(':') && !l.starts_with(' ') {
			// package key line
			current = Some(l.trim_end_matches(':').trim_matches('"').to_string());
			continue;
		}
		if l.trim_start().starts_with("dependencies:") { continue; }
		if let Some(cur) = &current {
			let t = l.trim_start();
			if t.starts_with(char::is_alphabetic) && t.contains(':') {
				let name = t.split(':').next().unwrap_or("").trim();
				if !name.is_empty() { edges.push((cur.clone(), name.to_string())); }
			}
		}
	}
	edges
}

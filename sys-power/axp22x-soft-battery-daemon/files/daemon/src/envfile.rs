//! A file of a run: lines `KEY=value`, in the order they were put.

use std::collections::HashMap;
use std::fmt::Display;

use crate::Result;

#[derive(Default)]
pub struct EnvFile {
    lines: Vec<(String, String)>,
    index: HashMap<String, usize>,
}

impl EnvFile {
    pub fn set(&mut self, key: &str, value: impl Display) {
        let value = value.to_string();
        match self.index.get(key) {
            Some(&i) => self.lines[i].1 = value,
            None => {
                self.index.insert(key.to_string(), self.lines.len());
                self.lines.push((key.to_string(), value));
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.index.get(key).map(|&i| self.lines[i].1.as_str())
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key)?.parse().ok()
    }

    /// A list of numbers, split by commas; none when the key is not there.
    pub fn get_list(&self, key: &str) -> Option<Vec<i64>> {
        let value = self.get(key)?;
        if value.is_empty() {
            return Some(Vec::new());
        }
        value.split(',').map(|v| v.parse().ok()).collect()
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut file = EnvFile::default();
        for (n, line) in text.lines().enumerate() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("line {}: no '='", n + 1))?;
            file.set(key, value);
        }
        Ok(file)
    }

    pub fn text(&self) -> String {
        self.lines.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
    }
}

pub fn join<T: Display>(values: &[T]) -> String {
    values.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")
}

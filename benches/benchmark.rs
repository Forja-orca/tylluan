//! # Benchmarks Module
//!
//! Simple benchmarks for critical TylluanNexus operations.
//! Run with: `cargo run --example bench`

use std::time::Instant;
use std::collections::HashMap;

pub fn measure<F, R>(name: &str, f: F) -> R
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();
    println!("⏱️  {}: {:?}", name, elapsed);
    result
}

pub fn bench_string_alloc() {
    measure("string_alloc_1k", || {
        let mut s = String::new();
        for i in 0..1000 {
            s.push_str("data_");
            s.push_str(&i.to_string());
        }
        s
    });
}

pub fn bench_vec_alloc() {
    measure("vec_alloc_10k", || {
        let mut v = Vec::with_capacity(10000);
        for i in 0..10000 {
            v.push(i * 2);
        }
        v
    });
}

pub fn bench_hashmap() {
    measure("hashmap_10k_insert", || {
        let mut map = HashMap::new();
        for i in 0..10_000 {
            map.insert(format!("key_{}", i), i * 2);
        }
        map
    });
}

pub fn bench_hashmap_lookup() {
    let mut map = HashMap::new();
    for i in 0..1000 {
        map.insert(format!("key_{}", i), i * 2);
    }
    
    measure("hashmap_1k_lookup", || {
        let mut sum = 0;
        for i in 0..1000 {
            sum += map.get(&format!("key_{}", i)).unwrap_or(&0);
        }
        sum
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_string_alloc() {
        bench_string_alloc();
    }
    
    #[test]
    fn test_vec_alloc() {
        bench_vec_alloc();
    }
    
    #[test]
    fn test_hashmap() {
        bench_hashmap();
    }
    
    #[test]
    fn test_hashmap_lookup() {
        bench_hashmap_lookup();
    }
}
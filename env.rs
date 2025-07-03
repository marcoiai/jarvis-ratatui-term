use std::env::*;
use std::collections::HashMap;

fn main() {
    let mut vars: HashMap<String, String> = HashMap::<String, String>::new();
    
    for (key, value) in std::env::vars() {
        vars.insert(key, value);
    }

    println!("{:?}", vars);

}

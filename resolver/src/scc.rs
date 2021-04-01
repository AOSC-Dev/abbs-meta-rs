use defaultmap::DefaultHashMap;
use std::cmp::min;

type DepGetter = dyn Fn(&str) -> Vec<String>;

pub struct TState {
    lowlink: DefaultHashMap<String, isize>,
    index: DefaultHashMap<String, isize>,
    stackstate: DefaultHashMap<String, bool>,
    stack: Vec<String>,
}

impl Default for TState {
    fn default() -> Self {
        TState {
            lowlink: DefaultHashMap::new(-1),
            stackstate: DefaultHashMap::new(false),
            stack: Vec::new(),
            index: DefaultHashMap::new(-1),
        }
    }
}

pub fn tarjan_search(packages: &[String], get_deps: &DepGetter) -> Vec<Vec<String>> {
    let mut state = TState::default();
    let mut results = Vec::new();
    for package in packages {
        if state.index[package.clone()] == -1 {
            let r = strongly_connected(package.to_string(), get_deps, &mut state, 0);
            results.extend(r);
        }
    }

    results
}

fn strongly_connected(
    v: String,
    get_deps: &DepGetter,
    state: &mut TState,
    depth: isize,
) -> Vec<Vec<String>> {
    let mut results = Vec::new();
    // update depth indices
    state.index[v.clone()] = depth;
    state.lowlink[v.clone()] = depth;
    let depth = depth + 1;
    state.stackstate[v.clone()] = true;
    state.stack.push(v.clone());
    let deps = get_deps(&v);
    // Look for adjacent nodes (dependencies)
    for d in deps {
        if state.index[d.clone()] == -1 {
            // recurse on unvisited packages
            let r = strongly_connected(d.clone(), get_deps, state, depth);
            results.extend(r);
            state.lowlink[v.clone()] = min(state.lowlink[d.clone()], state.lowlink[v.clone()]);
        } else if state.stackstate[d.clone()] {
            // adjacent package is in the stack which means it is part of a loop
            state.lowlink[v.clone()] = min(state.lowlink[d.clone()], state.index[v.clone()]);
        }
    }

    let mut w = String::new();
    let mut result = vec![];
    // if this is a root vertex
    if state.lowlink[v.clone()] == state.index[v.clone()] {
        while w != v {
            // the current stack contains the vertices that belong to the same loop
            // if the stack only contains one vertex, then there is no loop there
            w = state.stack.pop().unwrap_or_default();
            result.push(w.clone());
            state.stackstate[w.clone()] = false;
        }
        results.push(result);
    }

    results
}

#[test]
fn simple_sort_test() {
    let results = tarjan_search(&["a".to_string()], &|d| match d {
        "a" => vec!["b".to_string()],
        "b" => vec!["c".to_string(), "d".to_string()],
        _ => vec![],
    });
    assert_eq!(
        results,
        vec![
            vec!["c".to_string()],
            vec!["d".to_string()],
            vec!["b".to_string()],
            vec!["a".to_string()]
        ]
    )
}

use super::*;

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

#[test]
fn test_no_cycle_simple_chain() {
    let mut graph = DependencyGraph::new();
    graph.add_edge(id(1), id(2), DependencyType::Blocks);
    graph.add_edge(id(2), id(3), DependencyType::Blocks);

    assert!(!graph.would_create_cycle(id(3), id(4)));
    assert!(graph.validate_new_edge(id(3), id(4)).is_ok());
}

#[test]
fn test_cycle_detection() {
    let mut graph = DependencyGraph::new();
    graph.add_edge(id(1), id(2), DependencyType::Blocks);
    graph.add_edge(id(2), id(3), DependencyType::Blocks);

    // Adding 3->1 would create a cycle
    assert!(graph.would_create_cycle(id(3), id(1)));
    assert!(graph.validate_new_edge(id(3), id(1)).is_err());
}

#[test]
fn test_self_reference_rejected() {
    let graph = DependencyGraph::new();
    assert!(graph.validate_new_edge(id(1), id(1)).is_err());
}

#[test]
fn test_blockers_of() {
    let mut graph = DependencyGraph::new();
    graph.add_edge(id(1), id(3), DependencyType::Blocks);
    graph.add_edge(id(2), id(3), DependencyType::Blocks);

    let blockers = graph.blockers_of(id(3));
    assert_eq!(blockers.len(), 2);
}

#[test]
fn test_all_downstream() {
    let mut graph = DependencyGraph::new();
    graph.add_edge(id(1), id(2), DependencyType::Blocks);
    graph.add_edge(id(2), id(3), DependencyType::Blocks);
    graph.add_edge(id(2), id(4), DependencyType::Blocks);

    let downstream = graph.all_downstream(id(1));
    assert!(downstream.contains(&id(2)));
    assert!(downstream.contains(&id(3)));
    assert!(downstream.contains(&id(4)));
    assert!(!downstream.contains(&id(1)));
}

#[test]
fn test_from_edges() {
    let edges = vec![
        DependencyEdge {
            source: id(1),
            target: id(2),
            dep_type: DependencyType::Blocks,
        },
        DependencyEdge {
            source: id(2),
            target: id(3),
            dep_type: DependencyType::Blocks,
        },
    ];
    let graph = DependencyGraph::from_edges(&edges);
    assert!(!graph.would_create_cycle(id(3), id(4)));
    assert!(graph.would_create_cycle(id(3), id(1)));
}

// ─── topological_order ─────────────────

#[test]
fn topo_order_simple_chain() {
    let mut graph = DependencyGraph::new();
    graph.add_edge(id(1), id(2), DependencyType::Blocks);
    graph.add_edge(id(2), id(3), DependencyType::Blocks);

    let order = graph.topological_order(&[id(3), id(2), id(1)]).unwrap();
    assert_eq!(order, vec![id(1), id(2), id(3)]);
}

#[test]
fn topo_order_diamond() {
    // 1 -> 2 -> 4
    // 1 -> 3 -> 4
    let mut graph = DependencyGraph::new();
    graph.add_edge(id(1), id(2), DependencyType::Blocks);
    graph.add_edge(id(1), id(3), DependencyType::Blocks);
    graph.add_edge(id(2), id(4), DependencyType::Blocks);
    graph.add_edge(id(3), id(4), DependencyType::Blocks);

    let order = graph
        .topological_order(&[id(4), id(3), id(2), id(1)])
        .unwrap();
    // 1 must come before 2 and 3; 4 must come after both.
    let pos = |n: Uuid| order.iter().position(|&x| x == n).unwrap();
    assert!(pos(id(1)) < pos(id(2)));
    assert!(pos(id(1)) < pos(id(3)));
    assert!(pos(id(2)) < pos(id(4)));
    assert!(pos(id(3)) < pos(id(4)));
}

#[test]
fn topo_order_is_deterministic_across_repeated_calls() {
    let mut graph = DependencyGraph::new();
    graph.add_edge(id(1), id(4), DependencyType::Blocks);
    graph.add_edge(id(2), id(4), DependencyType::Blocks);
    graph.add_edge(id(3), id(4), DependencyType::Blocks);

    let nodes = [id(4), id(3), id(2), id(1)];
    let first = graph.topological_order(&nodes).unwrap();
    for _ in 0..20 {
        assert_eq!(graph.topological_order(&nodes).unwrap(), first);
    }
    // Ties (1, 2, 3 all have in-degree 0) break by input-slice position,
    // not by hash iteration order — so the order is exactly this, not
    // merely "some" valid order.
    assert_eq!(first, vec![id(3), id(2), id(1), id(4)]);
}

#[test]
fn topo_order_ignores_edges_to_nodes_outside_the_set() {
    // 1 -> 2, but only 2 is in the requested node set — 1's absence must
    // not affect 2's position or cause an error.
    let mut graph = DependencyGraph::new();
    graph.add_edge(id(1), id(2), DependencyType::Blocks);
    graph.add_edge(id(2), id(3), DependencyType::Blocks);

    let order = graph.topological_order(&[id(2), id(3)]).unwrap();
    assert_eq!(order, vec![id(2), id(3)]);
}

#[test]
fn topo_order_handles_disjoint_components() {
    let mut graph = DependencyGraph::new();
    graph.add_edge(id(1), id(2), DependencyType::Blocks);
    graph.add_edge(id(3), id(4), DependencyType::Blocks);

    let order = graph
        .topological_order(&[id(4), id(3), id(2), id(1)])
        .unwrap();
    assert_eq!(order.len(), 4);
    let pos = |n: Uuid| order.iter().position(|&x| x == n).unwrap();
    assert!(pos(id(1)) < pos(id(2)));
    assert!(pos(id(3)) < pos(id(4)));
}

#[test]
fn topo_order_fails_loudly_rather_than_hanging_on_an_impossible_cycle() {
    // Bypasses `validate_new_edge` (which is supposed to make this
    // structurally unreachable in production) to prove the function
    // detects the impossible case and returns an error instead of
    // looping forever or silently dropping the stuck nodes.
    let mut graph = DependencyGraph::new();
    graph.add_edge(id(1), id(2), DependencyType::Blocks);
    graph.add_edge(id(2), id(1), DependencyType::Blocks);

    let result = graph.topological_order(&[id(1), id(2)]);
    assert!(matches!(result, Err(CoreError::DependencyCycle(_))));
}

#[test]
fn topo_order_single_node_no_edges() {
    let graph = DependencyGraph::new();
    assert_eq!(graph.topological_order(&[id(1)]).unwrap(), vec![id(1)]);
}

#[test]
fn topo_order_empty_input() {
    let graph = DependencyGraph::new();
    assert_eq!(graph.topological_order(&[]).unwrap(), Vec::<Uuid>::new());
}

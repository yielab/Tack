use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use tracing::{debug, instrument};
use uuid::Uuid;

use crate::error::CoreError;
use crate::models::DependencyType;

/// Lightweight edge representation for the dependency graph.
#[derive(Debug, Clone)]
pub struct DependencyEdge {
    pub source: Uuid,
    pub target: Uuid,
    pub dep_type: DependencyType,
}

/// Dependency graph for cycle detection and blocked-item computation.
#[derive(Debug, Default)]
pub struct DependencyGraph {
    /// adjacency list: item -> items it blocks
    edges: HashMap<Uuid, Vec<(Uuid, DependencyType)>>,
    /// reverse adjacency: item -> items that block it
    reverse_edges: HashMap<Uuid, Vec<(Uuid, DependencyType)>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the graph from a list of edges.
    pub fn from_edges(edges: &[DependencyEdge]) -> Self {
        let mut graph = Self::new();
        for edge in edges {
            graph.add_edge(edge.source, edge.target, edge.dep_type.clone());
        }
        graph
    }

    /// Add a directed edge (source blocks target).
    pub fn add_edge(&mut self, source: Uuid, target: Uuid, dep_type: DependencyType) {
        self.edges
            .entry(source)
            .or_default()
            .push((target, dep_type.clone()));
        self.reverse_edges
            .entry(target)
            .or_default()
            .push((source, dep_type));
    }

    /// Check if adding an edge would create a cycle (DFS-based).
    #[instrument(skip(self), fields(source = %source, target = %target))]
    pub fn would_create_cycle(&self, source: Uuid, target: Uuid) -> bool {
        // If adding source->target, check if target can reach source
        let mut visited = HashSet::new();
        let mut stack = VecDeque::new();
        stack.push_back(target);

        while let Some(node) = stack.pop_back() {
            if node == source {
                debug!("Cycle detected: {target} can reach {source}");
                return true;
            }
            if visited.insert(node)
                && let Some(neighbors) = self.edges.get(&node)
            {
                for (neighbor, _) in neighbors {
                    stack.push_back(*neighbor);
                }
            }
        }

        false
    }

    /// Get all items that directly block the given item.
    pub fn blockers_of(&self, item_id: Uuid) -> Vec<(Uuid, DependencyType)> {
        self.reverse_edges
            .get(&item_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Get all items that the given item directly blocks.
    pub fn blocked_by(&self, item_id: Uuid) -> Vec<(Uuid, DependencyType)> {
        self.edges.get(&item_id).cloned().unwrap_or_default()
    }

    /// Get all transitively blocked items (BFS from source).
    pub fn all_downstream(&self, item_id: Uuid) -> HashSet<Uuid> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(item_id);

        while let Some(node) = queue.pop_front() {
            if let Some(neighbors) = self.edges.get(&node) {
                for (neighbor, _) in neighbors {
                    if visited.insert(*neighbor) {
                        queue.push_back(*neighbor);
                    }
                }
            }
        }

        visited
    }

    /// Validate that adding an edge won't create a cycle, returning an error if it would.
    pub fn validate_new_edge(&self, source: Uuid, target: Uuid) -> Result<(), CoreError> {
        if source == target {
            return Err(CoreError::DependencyCycle(source));
        }
        if self.would_create_cycle(source, target) {
            return Err(CoreError::DependencyCycle(target));
        }
        Ok(())
    }

    /// Topologically sort `nodes` (Kahn's algorithm), considering only edges
    /// whose **both** endpoints are in `nodes` — an edge to a node outside the
    /// given set contributes nothing to the ordering of the given set (the
    /// caller — [`crate::dependency`]'s sprint-dispatch consumer, at the time
    /// of writing — is expected to check readiness against out-of-set
    /// dependencies separately, since "is my blocker done" and "in what order
    /// do I sort the nodes I'm about to act on" are different questions).
    ///
    /// **Deterministic**, not just "a" valid order: among nodes that become
    /// ready to emit at the same time, the one that appears earliest in the
    /// input `nodes` slice is emitted first. This matters because a caller —
    /// like a dry-run preview that must match a real run's order exactly —
    /// calls this twice for the same input and needs the same answer both
    /// times, and `HashMap`/`HashSet` iteration order is randomized per
    /// instance in this codebase's default hasher, so it cannot be the
    /// tie-breaker.
    ///
    /// Returns `Err(CoreError::DependencyCycle(node))` if `nodes` cannot be
    /// fully ordered. This should be structurally unreachable — every edge is
    /// validated against [`Self::validate_new_edge`] before insertion — so a
    /// caller reaching this branch has a real invariant violation and should
    /// fail loudly (a 500, an assertion) rather than silently truncate the
    /// order or hang waiting on a dependency that can never be satisfied.
    pub fn topological_order(&self, nodes: &[Uuid]) -> Result<Vec<Uuid>, CoreError> {
        let node_set: HashSet<Uuid> = nodes.iter().copied().collect();
        let node_index: HashMap<Uuid, usize> =
            nodes.iter().enumerate().map(|(i, &n)| (n, i)).collect();

        let mut in_degree: HashMap<Uuid, usize> = node_set.iter().map(|&n| (n, 0)).collect();
        let mut adjacency: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for &n in &node_set {
            let Some(targets) = self.edges.get(&n) else {
                continue;
            };
            for (target, _) in targets {
                if node_set.contains(target) {
                    adjacency.entry(n).or_default().push(*target);
                    *in_degree.entry(*target).or_insert(0) += 1;
                }
            }
        }

        // Min-heap keyed by each node's position in the original `nodes`
        // slice — the deterministic tie-breaker described above.
        let mut ready: BinaryHeap<Reverse<(usize, Uuid)>> = nodes
            .iter()
            .filter(|n| in_degree.get(n).copied().unwrap_or(0) == 0)
            .map(|&n| Reverse((node_index[&n], n)))
            .collect();

        let mut order = Vec::with_capacity(node_set.len());
        while let Some(Reverse((_, n))) = ready.pop() {
            order.push(n);
            if let Some(targets) = adjacency.get(&n) {
                for &t in targets {
                    let deg = in_degree
                        .get_mut(&t)
                        .expect("target has an in-degree entry");
                    *deg -= 1;
                    if *deg == 0 {
                        ready.push(Reverse((node_index[&t], t)));
                    }
                }
            }
        }

        if order.len() != node_set.len() {
            let stuck = nodes
                .iter()
                .find(|n| !order.contains(n))
                .copied()
                .unwrap_or(nodes[0]);
            return Err(CoreError::DependencyCycle(stuck));
        }

        Ok(order)
    }
}

#[cfg(test)]
#[path = "dependency/tests.rs"]
mod tests;

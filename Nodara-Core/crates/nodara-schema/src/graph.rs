//! Graph algorithms over a [`Workflow`].
//!
//! These are pure functions over an indexed view of the workflow. Keeping them
//! in `nodara-schema` (rather than the engine) lets the editor, the CLI and the
//! agent share exactly one notion of topology.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::workflow::{Edge, Node, Workflow};

/// Errors produced by graph algorithms.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    /// The graph contains a cycle and a topological order does not exist.
    #[error("cycle detected involving: {0}")]
    Cycle(String),
    /// A referenced node does not exist.
    #[error("node not found: {0}")]
    NodeNotFound(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Colour {
    White,
    Grey,
    Black,
}

/// Indexed, read-only view over a workflow graph.
#[derive(Debug)]
pub struct WorkflowGraph<'a> {
    workflow: &'a Workflow,
    index: HashMap<&'a str, &'a Node>,
    outgoing: HashMap<&'a str, Vec<&'a Edge>>,
    incoming: HashMap<&'a str, Vec<&'a Edge>>,
}

impl<'a> WorkflowGraph<'a> {
    /// Build an indexed view. Duplicate ids resolve to the first occurrence,
    /// which mirrors how validation reports the problem.
    pub fn new(workflow: &'a Workflow) -> Self {
        let mut index = HashMap::with_capacity(workflow.nodes.len());
        for node in &workflow.nodes {
            index.entry(node.id.as_str()).or_insert(node);
        }

        let mut outgoing: HashMap<&str, Vec<&Edge>> = HashMap::new();
        let mut incoming: HashMap<&str, Vec<&Edge>> = HashMap::new();
        for edge in &workflow.edges {
            outgoing.entry(edge.source.as_str()).or_default().push(edge);
            incoming.entry(edge.target.as_str()).or_default().push(edge);
        }

        Self {
            workflow,
            index,
            outgoing,
            incoming,
        }
    }

    /// The underlying workflow.
    pub fn workflow(&self) -> &'a Workflow {
        self.workflow
    }

    /// Look up a node by id.
    pub fn node(&self, id: &str) -> Option<&'a Node> {
        self.index.get(id).copied()
    }

    /// Position of a node in the original `nodes` array.
    pub fn node_position(&self, id: &str) -> Option<usize> {
        self.workflow.nodes.iter().position(|n| n.id == id)
    }

    /// Outgoing edges of a node.
    pub fn edges_from(&self, id: &str) -> &[&'a Edge] {
        self.outgoing.get(id).map_or(&[], Vec::as_slice)
    }

    /// Incoming edges of a node.
    pub fn edges_to(&self, id: &str) -> &[&'a Edge] {
        self.incoming.get(id).map_or(&[], Vec::as_slice)
    }

    /// Number of outgoing edges.
    pub fn out_degree(&self, id: &str) -> usize {
        self.outgoing.get(id).map_or(0, Vec::len)
    }

    /// Number of incoming edges.
    pub fn in_degree(&self, id: &str) -> usize {
        self.incoming.get(id).map_or(0, Vec::len)
    }

    /// Nodes with no incoming edges.
    pub fn roots(&self) -> Vec<&'a Node> {
        self.workflow
            .nodes
            .iter()
            .filter(|n| self.in_degree(&n.id) == 0)
            .collect()
    }

    /// Nodes with no outgoing edges.
    pub fn sinks(&self) -> Vec<&'a Node> {
        self.workflow
            .nodes
            .iter()
            .filter(|n| self.out_degree(&n.id) == 0)
            .collect()
    }

    /// The preferred entry point: the unique `core.Start`, else the first root.
    pub fn entry_node(&self) -> Option<&'a Node> {
        self.workflow
            .nodes
            .iter()
            .find(|n| n.node_type == "core.Start")
            .or_else(|| self.roots().into_iter().next())
    }

    /// All node ids reachable from `start`.
    pub fn reachable_from(&self, start: &str) -> HashSet<&'a str> {
        let mut visited: HashSet<&str> = HashSet::new();
        let Some(start_node) = self.node(start) else {
            return visited;
        };
        let mut queue: VecDeque<&str> = VecDeque::new();
        visited.insert(start_node.id.as_str());
        queue.push_back(start_node.id.as_str());
        while let Some(current) = queue.pop_front() {
            for edge in self.edges_from(current) {
                if let Some(target) = self.index.get(edge.target.as_str()) {
                    if visited.insert(target.id.as_str()) {
                        queue.push_back(target.id.as_str());
                    }
                }
            }
        }
        visited
    }

    /// Kahn topological order. Returns [`GraphError::Cycle`] when the graph is
    /// not a DAG, naming the nodes that could not be scheduled.
    pub fn topological_order(&self) -> Result<Vec<&'a str>, GraphError> {
        let mut in_degree: HashMap<&str, usize> = HashMap::with_capacity(self.workflow.nodes.len());
        for node in &self.workflow.nodes {
            in_degree.entry(node.id.as_str()).or_insert(0);
        }
        for edge in &self.workflow.edges {
            if self.index.contains_key(edge.target.as_str())
                && self.index.contains_key(edge.source.as_str())
            {
                *in_degree.entry(edge.target.as_str()).or_insert(0) += 1;
            }
        }

        // Preserve declaration order among ready nodes for deterministic output.
        let mut ready: VecDeque<&str> = self
            .workflow
            .nodes
            .iter()
            .filter(|n| in_degree.get(n.id.as_str()).copied().unwrap_or(0) == 0)
            .map(|n| n.id.as_str())
            .collect();

        let mut scheduled: HashSet<&str> = HashSet::with_capacity(self.workflow.nodes.len());
        let mut order = Vec::with_capacity(self.workflow.nodes.len());
        while let Some(current) = ready.pop_front() {
            order.push(current);
            scheduled.insert(current);
            for edge in self.edges_from(current) {
                let target = edge.target.as_str();
                let Some(degree) = in_degree.get_mut(target) else {
                    continue;
                };
                if *degree == 0 {
                    continue;
                }
                *degree -= 1;
                if *degree == 0 {
                    ready.push_back(target);
                }
            }
        }

        if order.len() == self.index.len() {
            Ok(order)
        } else {
            let unscheduled = self
                .workflow
                .nodes
                .iter()
                .filter(|n| !scheduled.contains(n.id.as_str()))
                .map(|n| n.id.clone())
                .collect::<Vec<_>>();
            Err(GraphError::Cycle(unscheduled.join(", ")))
        }
    }

    /// Find one cycle, if any.
    pub fn find_cycle(&self) -> Option<Vec<String>> {
        let mut colour: HashMap<&str, Colour> = self
            .workflow
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), Colour::White))
            .collect();
        let mut stack: Vec<&str> = Vec::new();

        for node in &self.workflow.nodes {
            if colour.get(node.id.as_str()).copied() == Some(Colour::White) {
                if let Some(cycle) = self.dfs_cycle(node.id.as_str(), &mut colour, &mut stack) {
                    return Some(cycle);
                }
            }
        }
        None
    }

    fn dfs_cycle(
        &self,
        node: &'a str,
        colour: &mut HashMap<&'a str, Colour>,
        stack: &mut Vec<&'a str>,
    ) -> Option<Vec<String>> {
        colour.insert(node, Colour::Grey);
        stack.push(node);
        for edge in self.edges_from(node) {
            let Some(target) = self.node(&edge.target) else {
                continue;
            };
            let target_id = target.id.as_str();
            match colour.get(target_id).copied().unwrap_or(Colour::White) {
                Colour::Grey => {
                    let start = stack.iter().position(|n| *n == target_id).unwrap_or(0);
                    let mut cycle: Vec<String> =
                        stack[start..].iter().map(|s| (*s).to_string()).collect();
                    cycle.push(target_id.to_string());
                    return Some(cycle);
                }
                Colour::White => {
                    if let Some(cycle) = self.dfs_cycle(target_id, colour, stack) {
                        return Some(cycle);
                    }
                }
                Colour::Black => {}
            }
        }
        stack.pop();
        colour.insert(node, Colour::Black);
        None
    }

    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.workflow.nodes.len()
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.workflow.edges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{Edge, Node, Workflow};

    fn sample() -> Workflow {
        let mut wf = Workflow::new("wf.graph");
        wf.add_node(Node::new("start", "core.Start"));
        wf.add_node(Node::new("a", "core.Log"));
        wf.add_node(Node::new("b", "core.Log"));
        wf.add_node(Node::new("end", "core.End"));
        wf.add_edge(Edge::new("e1", "start", "a"));
        wf.add_edge(Edge::new("e2", "start", "b"));
        wf.add_edge(Edge::new("e3", "a", "end"));
        wf.add_edge(Edge::new("e4", "b", "end"));
        wf
    }

    #[test]
    fn computes_topological_order() {
        let wf = sample();
        let graph = WorkflowGraph::new(&wf);
        let order = graph.topological_order().expect("acyclic");
        assert_eq!(order.first().copied(), Some("start"));
        assert_eq!(order.last().copied(), Some("end"));
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn detects_cycle_in_topological_order() {
        let mut wf = sample();
        wf.add_edge(Edge::new("e5", "end", "a"));
        let graph = WorkflowGraph::new(&wf);
        assert!(matches!(
            graph.topological_order(),
            Err(GraphError::Cycle(_))
        ));
        assert!(graph.find_cycle().is_some());
    }

    #[test]
    fn acyclic_graph_has_no_cycle() {
        let wf = sample();
        assert!(WorkflowGraph::new(&wf).find_cycle().is_none());
    }

    #[test]
    fn entry_node_prefers_core_start() {
        let wf = sample();
        let graph = WorkflowGraph::new(&wf);
        assert_eq!(graph.entry_node().unwrap().id, "start");
    }

    #[test]
    fn reachability_is_transitive() {
        let wf = sample();
        let graph = WorkflowGraph::new(&wf);
        let reachable = graph.reachable_from("start");
        assert_eq!(reachable.len(), 4);
        assert!(reachable.contains("end"));
    }
}

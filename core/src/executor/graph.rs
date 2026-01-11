use std::collections::{HashMap, HashSet};

use crate::error::ExecutorError;
use crate::executor::types::TaskLike;

/// Task dependency graph (DAG)
#[derive(Debug, Clone)]
pub struct TaskGraph<T: TaskLike> {
    /// Task nodes: task_id -> Task
    pub nodes: HashMap<String, T>,

    /// Dependency edges: task_id -> list of dependencies
    pub edges: HashMap<String, Vec<String>>,

    /// Reverse edges: task_id -> list of tasks that depend on it
    pub reverse_edges: HashMap<String, Vec<String>>,

    /// Original insertion order (for stable sorting)
    insertion_order: Vec<String>,
}

impl<T: TaskLike> TaskGraph<T> {
    /// Construct task graph from task list
    pub fn from_tasks(tasks: Vec<T>) -> Result<Self, ExecutorError> {
        let mut nodes = HashMap::new();
        let mut edges = HashMap::new();
        let mut reverse_edges: HashMap<String, Vec<String>> = HashMap::new();
        let mut insertion_order = Vec::new();

        // Add all nodes
        for task in tasks {
            if nodes.contains_key(task.id()) {
                return Err(ExecutorError::DuplicateTaskId(task.id().to_string()));
            }

            let task_id = task.id().to_string();
            let dependencies = task.dependencies().to_vec();

            nodes.insert(task_id.clone(), task);
            edges.insert(task_id.clone(), dependencies.clone());
            insertion_order.push(task_id.clone());

            // Build reverse edges
            for dep in dependencies {
                reverse_edges
                    .entry(dep)
                    .or_default()
                    .push(task_id.clone());
            }
        }

        Ok(Self {
            nodes,
            edges,
            reverse_edges,
            insertion_order,
        })
    }

    /// Validate dependency relationships
    pub fn validate(&self) -> Result<(), ExecutorError> {
        // Check all dependencies exist
        for (task_id, dependencies) in &self.edges {
            for dep in dependencies {
                if !self.nodes.contains_key(dep) {
                    return Err(ExecutorError::DependencyNotFound {
                        task_id: task_id.clone(),
                        missing_dep: dep.clone(),
                    });
                }
            }
        }

        // Detect circular dependencies
        if let Some(cycle) = self.detect_cycle() {
            return Err(ExecutorError::CircularDependency(cycle));
        }

        Ok(())
    }

    /// Topological sort using Kahn's algorithm
    ///
    /// Returns execution stages where tasks in the same stage can run in parallel.
    ///
    /// # Algorithm
    ///
    /// 1. Calculate in-degree for all nodes
    /// 2. Find all nodes with in-degree 0 (first stage)
    /// 3. Remove these nodes and update in-degrees
    /// 4. Repeat until all nodes processed
    ///
    /// # Time Complexity
    ///
    /// O(V + E) where V = number of tasks, E = number of dependencies
    pub fn topological_sort(&self) -> Result<Vec<Vec<String>>, ExecutorError> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        // Initialize in-degrees to 0
        for task_id in self.nodes.keys() {
            in_degree.insert(task_id.clone(), 0);
        }

        // Calculate in-degrees
        // edges[A] = [B, C] means A depends on B and C
        // In execution graph: B -> A, C -> A
        // So A's in-degree = 2
        for (task_id, dependencies) in &self.edges {
            *in_degree.get_mut(task_id).unwrap() += dependencies.len();
        }

        // Find all nodes with in-degree 0 (first stage)
        let mut stages: Vec<Vec<String>> = Vec::new();
        let mut current_stage: Vec<String> = in_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(id, _)| id.clone())
            .collect();

        // Preserve input order for stable output
        current_stage.sort_by_key(|id| {
            self.insertion_order
                .iter()
                .position(|k| k == id)
                .unwrap_or(usize::MAX)
        });

        let mut processed = 0;

        // Process stages
        while !current_stage.is_empty() {
            stages.push(current_stage.clone());
            processed += current_stage.len();

            // Update in-degrees and find next stage
            let mut next_stage = Vec::new();

            for task_id in &current_stage {
                if let Some(dependents) = self.reverse_edges.get(task_id) {
                    for dependent in dependents {
                        let degree = in_degree.get_mut(dependent).unwrap();
                        *degree -= 1;

                        if *degree == 0 {
                            next_stage.push(dependent.clone());
                        }
                    }
                }
            }

            // Preserve input order
            next_stage.sort_by_key(|id| {
                self.insertion_order
                    .iter()
                    .position(|k| k == id)
                    .unwrap_or(usize::MAX)
            });

            current_stage = next_stage;
        }

        // Verify all nodes processed (no cycles)
        if processed != self.nodes.len() {
            return Err(ExecutorError::CircularDependency(
                "Unable to complete topological sort (cycle detected)".to_string(),
            ));
        }

        Ok(stages)
    }

    /// Detect circular dependencies using DFS
    ///
    /// # Time Complexity
    ///
    /// O(V + E) where V = number of tasks, E = number of dependencies
    fn detect_cycle(&self) -> Option<String> {
        let mut visited = HashSet::new();
        let mut stack = Vec::new();

        for task_id in self.nodes.keys() {
            if !visited.contains(task_id)
                && self.dfs_cycle(task_id, &mut visited, &mut stack) {
                    return Some(format_cycle_path(&stack));
                }
        }

        None
    }

    fn dfs_cycle(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        stack: &mut Vec<String>,
    ) -> bool {
        visited.insert(node.to_string());
        stack.push(node.to_string());

        if let Some(dependencies) = self.edges.get(node) {
            for dep in dependencies {
                // Check if dependency is in current path (cycle detected)
                if let Some(pos) = stack.iter().position(|x| x == dep) {
                    stack.push(dep.clone());
                    *stack = stack[pos..].to_vec();
                    return true;
                }

                // Recursively check unvisited dependencies
                if !visited.contains(dep)&& self.dfs_cycle(dep, visited, stack) {
                    return true;
                }
            }
        }

        stack.pop();
        false
    }
}

fn format_cycle_path(stack: &[String]) -> String {
    stack.join(" -> ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stdio::StdioTask;
    use crate::stdio::{FilesEncoding, FilesMode};

    fn task(id: &str, deps: &[&str]) -> StdioTask {
        StdioTask {
            id: id.to_string(),
            backend: "codex".to_string(),
            workdir: ".".to_string(),
            model: None,
            model_provider: None,
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            stream_format: "text".to_string(),
            timeout: None,
            retry: None,
            files: vec![],
            files_mode: FilesMode::Auto,
            files_encoding: FilesEncoding::Auto,
            content: String::new(),
        }
    }

    #[test]
    fn test_topological_sort_linear() {
        // A -> B -> C
        let tasks = vec![task("A", &[]), task("B", &["A"]), task("C", &["B"])];

        let graph = TaskGraph::from_tasks(tasks).unwrap();
        graph.validate().unwrap();
        let stages = graph.topological_sort().unwrap();

        assert_eq!(
            stages,
            vec![
                vec!["A".to_string()],
                vec!["B".to_string()],
                vec!["C".to_string()]
            ]
        );
    }

    #[test]
    fn test_topological_sort_diamond() {
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        let tasks = vec![
            task("A", &[]),
            task("B", &["A"]),
            task("C", &["A"]),
            task("D", &["B", "C"]),
        ];

        let graph = TaskGraph::from_tasks(tasks).unwrap();
        graph.validate().unwrap();
        let stages = graph.topological_sort().unwrap();

        assert_eq!(stages.len(), 3);
        assert_eq!(stages[0], vec!["A".to_string()]);
        assert_eq!(stages[1], vec!["B".to_string(), "C".to_string()]);
        assert_eq!(stages[2], vec!["D".to_string()]);
    }

    #[test]
    fn test_detect_cycle_simple() {
        // A -> B -> A
        let tasks = vec![task("A", &["B"]), task("B", &["A"])];

        let graph = TaskGraph::from_tasks(tasks).unwrap();
        let result = graph.validate();

        assert!(result.is_err());
        match result.unwrap_err() {
            ExecutorError::CircularDependency(msg) => {
                assert!(msg.contains("A") && msg.contains("B"));
            }
            _ => panic!("Expected CircularDependency error"),
        }
    }

    #[test]
    fn test_detect_cycle_complex() {
        // A -> B -> C -> D -> B (cycle)
        let tasks = vec![
            task("A", &[]),
            task("B", &["A"]),
            task("C", &["B"]),
            task("D", &["C"]),
            task("E", &["D", "B"]),
        ];

        let graph = TaskGraph::from_tasks(tasks).unwrap();
        // This should NOT have a cycle (E depends on both D and B, but no back edge)
        assert!(graph.validate().is_ok());

        // Create actual cycle: D -> B
        let tasks_with_cycle = vec![
            task("A", &[]),
            task("B", &["A", "D"]), // Cycle: B -> A -> B via D
            task("C", &["B"]),
            task("D", &["C"]),
        ];

        let graph = TaskGraph::from_tasks(tasks_with_cycle).unwrap();
        assert!(graph.validate().is_err());
    }

    #[test]
    fn test_missing_dependency() {
        let tasks = vec![task("A", &["B"])]; // B doesn't exist

        let graph = TaskGraph::from_tasks(tasks).unwrap();
        let result = graph.validate();

        assert!(result.is_err());
        match result.unwrap_err() {
            ExecutorError::DependencyNotFound {
                task_id,
                missing_dep,
            } => {
                assert_eq!(task_id, "A");
                assert_eq!(missing_dep, "B");
            }
            _ => panic!("Expected DependencyNotFound error"),
        }
    }

    #[test]
    fn test_duplicate_task_id() {
        let tasks = vec![task("A", &[]), task("A", &[])];

        let result = TaskGraph::from_tasks(tasks);

        assert!(result.is_err());
        match result.unwrap_err() {
            ExecutorError::DuplicateTaskId(id) => {
                assert_eq!(id, "A");
            }
            _ => panic!("Expected DuplicateTaskId error"),
        }
    }

    #[test]
    fn test_single_layer_preserves_input_order() {
        let tasks = vec![task("C", &[]), task("A", &[]), task("B", &[])];

        let graph = TaskGraph::from_tasks(tasks).unwrap();
        let stages = graph.topological_sort().unwrap();

        assert_eq!(stages.len(), 1);
        assert_eq!(
            stages[0],
            vec!["C".to_string(), "A".to_string(), "B".to_string()]
        );
    }
}

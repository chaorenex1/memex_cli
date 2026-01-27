/// 深度优先搜索（DFS）算法示例
///
/// 这个示例展示了 DFS 算法的多种实现方式和应用场景

use std::collections::{HashMap, HashSet};

/// 图的邻接表表示
type Graph = HashMap<usize, Vec<usize>>;

/// 1. 递归实现 DFS（最常见的方式）
fn dfs_recursive(graph: &Graph, node: usize, visited: &mut HashSet<usize>, path: &mut Vec<usize>) {
    visited.insert(node);
    path.push(node);

    if let Some(neighbors) = graph.get(&node) {
        for &neighbor in neighbors {
            if !visited.contains(&neighbor) {
                dfs_recursive(graph, neighbor, visited, path);
            }
        }
    }
}

/// 2. 迭代实现 DFS（使用显式栈）
fn dfs_iterative(graph: &Graph, start: usize) -> Vec<usize> {
    let mut visited = HashSet::new();
    let mut stack = vec![start];
    let mut path = Vec::new();

    while let Some(node) = stack.pop() {
        if visited.contains(&node) {
            continue;
        }

        visited.insert(node);
        path.push(node);

        // 将相邻节点压入栈（逆序以保持访问顺序）
        if let Some(neighbors) = graph.get(&node) {
            for &neighbor in neighbors.iter().rev() {
                if !visited.contains(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }
    }

    path
}

/// 3. 路径查找：找到从起点到终点的一条路径
fn find_path(graph: &Graph, start: usize, target: usize) -> Option<Vec<usize>> {
    let mut visited = HashSet::new();
    let mut path = Vec::new();

    fn dfs_path(
        graph: &Graph,
        current: usize,
        target: usize,
        visited: &mut HashSet<usize>,
        path: &mut Vec<usize>,
    ) -> bool {
        visited.insert(current);
        path.push(current);

        if current == target {
            return true;
        }

        if let Some(neighbors) = graph.get(&current) {
            for &neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    if dfs_path(graph, neighbor, target, visited, path) {
                        return true;
                    }
                }
            }
        }

        path.pop();
        false
    }

    if dfs_path(graph, start, target, &mut visited, &mut path) {
        Some(path)
    } else {
        None
    }
}

/// 4. 环检测：检测图中是否存在环
fn has_cycle(graph: &Graph) -> bool {
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();

    fn dfs_cycle(
        graph: &Graph,
        node: usize,
        visited: &mut HashSet<usize>,
        rec_stack: &mut HashSet<usize>,
    ) -> bool {
        visited.insert(node);
        rec_stack.insert(node);

        if let Some(neighbors) = graph.get(&node) {
            for &neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    if dfs_cycle(graph, neighbor, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(&neighbor) {
                    // 发现环
                    return true;
                }
            }
        }

        rec_stack.remove(&node);
        false
    }

    // 检查所有未访问的节点（处理非连通图）
    for &node in graph.keys() {
        if !visited.contains(&node) {
            if dfs_cycle(graph, node, &mut visited, &mut rec_stack) {
                return true;
            }
        }
    }

    false
}

/// 5. 拓扑排序（适用于有向无环图）
fn topological_sort(graph: &Graph) -> Option<Vec<usize>> {
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut result = Vec::new();

    fn dfs_topo(
        graph: &Graph,
        node: usize,
        visited: &mut HashSet<usize>,
        rec_stack: &mut HashSet<usize>,
        result: &mut Vec<usize>,
    ) -> bool {
        visited.insert(node);
        rec_stack.insert(node);

        if let Some(neighbors) = graph.get(&node) {
            for &neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    if !dfs_topo(graph, neighbor, visited, rec_stack, result) {
                        return false;
                    }
                } else if rec_stack.contains(&neighbor) {
                    // 发现环，不能进行拓扑排序
                    return false;
                }
            }
        }

        rec_stack.remove(&node);
        result.push(node);
        true
    }

    for &node in graph.keys() {
        if !visited.contains(&node) {
            if !dfs_topo(graph, node, &mut visited, &mut rec_stack, &mut result) {
                return None; // 图中有环
            }
        }
    }

    result.reverse();
    Some(result)
}

/// 6. 连通分量：找出图中的所有连通子图
fn find_connected_components(graph: &Graph) -> Vec<Vec<usize>> {
    let mut visited = HashSet::new();
    let mut components = Vec::new();

    for &node in graph.keys() {
        if !visited.contains(&node) {
            let mut component = Vec::new();
            dfs_recursive(graph, node, &mut visited, &mut component);
            components.push(component);
        }
    }

    components
}

/// 7. 迷宫求解（网格图 DFS）
fn solve_maze(maze: &Vec<Vec<i32>>, start: (usize, usize), end: (usize, usize)) -> Option<Vec<(usize, usize)>> {
    let rows = maze.len();
    let cols = maze[0].len();
    let mut visited = vec![vec![false; cols]; rows];
    let mut path = Vec::new();

    fn dfs_maze(
        maze: &Vec<Vec<i32>>,
        pos: (usize, usize),
        end: (usize, usize),
        visited: &mut Vec<Vec<bool>>,
        path: &mut Vec<(usize, usize)>,
    ) -> bool {
        let (row, col) = pos;
        let rows = maze.len();
        let cols = maze[0].len();

        // 边界检查和障碍物检查
        if row >= rows || col >= cols || maze[row][col] == 1 || visited[row][col] {
            return false;
        }

        visited[row][col] = true;
        path.push(pos);

        if pos == end {
            return true;
        }

        // 四个方向：上、右、下、左
        let directions = [(0, 1), (1, 0), (0, -1), (-1, 0)];
        for (dr, dc) in directions.iter() {
            let new_row = row as i32 + dr;
            let new_col = col as i32 + dc;

            if new_row >= 0 && new_col >= 0 {
                if dfs_maze(
                    maze,
                    (new_row as usize, new_col as usize),
                    end,
                    visited,
                    path,
                ) {
                    return true;
                }
            }
        }

        path.pop();
        false
    }

    if dfs_maze(maze, start, end, &mut visited, &mut path) {
        Some(path)
    } else {
        None
    }
}

fn main() {
    println!("=== 深度优先搜索（DFS）算法示例 ===\n");

    // 创建一个示例图
    //     0
    //    / \
    //   1   2
    //  / \   \
    // 3   4   5
    let mut graph: Graph = HashMap::new();
    graph.insert(0, vec![1, 2]);
    graph.insert(1, vec![3, 4]);
    graph.insert(2, vec![5]);
    graph.insert(3, vec![]);
    graph.insert(4, vec![]);
    graph.insert(5, vec![]);

    // 1. 递归 DFS
    println!("1. 递归 DFS 遍历:");
    let mut visited = HashSet::new();
    let mut path = Vec::new();
    dfs_recursive(&graph, 0, &mut visited, &mut path);
    println!("   遍历路径: {:?}\n", path);

    // 2. 迭代 DFS
    println!("2. 迭代 DFS 遍历:");
    let path = dfs_iterative(&graph, 0);
    println!("   遍历路径: {:?}\n", path);

    // 3. 路径查找
    println!("3. 路径查找（从 0 到 5）:");
    if let Some(path) = find_path(&graph, 0, 5) {
        println!("   找到路径: {:?}\n", path);
    }

    // 4. 环检测
    println!("4. 环检测:");
    let mut cyclic_graph: Graph = HashMap::new();
    cyclic_graph.insert(0, vec![1]);
    cyclic_graph.insert(1, vec![2]);
    cyclic_graph.insert(2, vec![0]); // 创建一个环

    println!("   无环图有环吗? {}", has_cycle(&graph));
    println!("   有环图有环吗? {}\n", has_cycle(&cyclic_graph));

    // 5. 拓扑排序
    println!("5. 拓扑排序（课程依赖关系）:");
    let mut dag: Graph = HashMap::new();
    // 课程依赖：5 -> 2 -> 3 -> 1, 4 -> 0 -> 1
    dag.insert(5, vec![2, 0]);
    dag.insert(4, vec![0, 1]);
    dag.insert(2, vec![3]);
    dag.insert(3, vec![1]);
    dag.insert(1, vec![]);
    dag.insert(0, vec![1]);

    if let Some(order) = topological_sort(&dag) {
        println!("   拓扑排序结果: {:?}", order);
        println!("   （学习顺序：从左到右）\n");
    }

    // 6. 连通分量
    println!("6. 查找连通分量:");
    let mut disconnected_graph: Graph = HashMap::new();
    disconnected_graph.insert(0, vec![1]);
    disconnected_graph.insert(1, vec![0]);
    disconnected_graph.insert(2, vec![3]);
    disconnected_graph.insert(3, vec![2]);
    disconnected_graph.insert(4, vec![5]);
    disconnected_graph.insert(5, vec![4]);

    let components = find_connected_components(&disconnected_graph);
    println!("   找到 {} 个连通分量:", components.len());
    for (i, component) in components.iter().enumerate() {
        println!("   分量 {}: {:?}", i + 1, component);
    }
    println!();

    // 7. 迷宫求解
    println!("7. 迷宫求解:");
    let maze = vec![
        vec![0, 0, 0, 0, 0],
        vec![1, 1, 0, 1, 0],
        vec![0, 0, 0, 0, 0],
        vec![0, 1, 1, 1, 0],
        vec![0, 0, 0, 0, 0],
    ];

    println!("   迷宫布局 (0=路径, 1=墙):");
    for row in &maze {
        print!("   ");
        for &cell in row {
            print!("{} ", cell);
        }
        println!();
    }

    let start = (0, 0);
    let end = (4, 4);
    println!("\n   起点: {:?}, 终点: {:?}", start, end);

    if let Some(path) = solve_maze(&maze, start, end) {
        println!("   找到路径（长度 {}）:", path.len());
        println!("   路径: {:?}", path);
    } else {
        println!("   未找到路径");
    }

    println!("\n=== DFS 算法示例完成 ===");
}

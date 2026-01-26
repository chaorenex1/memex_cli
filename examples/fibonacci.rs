/// 斐波那契数列的多种实现方式
///
/// 这个示例展示了在 Rust 中实现斐波那契数列的几种不同方法

/// 1. 递归实现（简单但效率低，适合小数值）
fn fibonacci_recursive(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci_recursive(n - 1) + fibonacci_recursive(n - 2),
    }
}

/// 2. 迭代实现（高效，推荐使用）
fn fibonacci_iterative(n: u32) -> u64 {
    if n == 0 {
        return 0;
    }

    let mut prev = 0u64;
    let mut curr = 1u64;

    for _ in 1..n {
        let next = prev + curr;
        prev = curr;
        curr = next;
    }

    curr
}

/// 3. 迭代器实现（Rust 风格，可以生成无限序列）
struct FibonacciIterator {
    prev: u64,
    curr: u64,
}

impl FibonacciIterator {
    fn new() -> Self {
        FibonacciIterator { prev: 0, curr: 1 }
    }
}

impl Iterator for FibonacciIterator {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.prev + self.curr;
        self.prev = self.curr;
        self.curr = next;
        Some(self.prev)
    }
}

/// 4. 使用记忆化的递归实现（平衡了递归的清晰性和性能）
fn fibonacci_memoized(n: u32) -> u64 {
    let mut memo = vec![None; (n + 1) as usize];
    fibonacci_memo_helper(n, &mut memo)
}

fn fibonacci_memo_helper(n: u32, memo: &mut Vec<Option<u64>>) -> u64 {
    if let Some(result) = memo[n as usize] {
        return result;
    }

    let result = match n {
        0 => 0,
        1 => 1,
        _ => fibonacci_memo_helper(n - 1, memo) + fibonacci_memo_helper(n - 2, memo),
    };

    memo[n as usize] = Some(result);
    result
}

/// 5. 尾递归优化版本
fn fibonacci_tail_recursive(n: u32) -> u64 {
    fn fib_helper(n: u32, prev: u64, curr: u64) -> u64 {
        if n == 0 {
            prev
        } else {
            fib_helper(n - 1, curr, prev + curr)
        }
    }

    fib_helper(n, 0, 1)
}

fn main() {
    println!("=== 斐波那契数列示例 ===\n");

    // 使用迭代方式计算前 20 个斐波那契数
    println!("前 20 个斐波那契数（迭代方式）:");
    for i in 0..20 {
        print!("{} ", fibonacci_iterative(i));
    }
    println!("\n");

    // 使用迭代器方式
    println!("使用迭代器生成前 15 个斐波那契数:");
    let fib_iter = FibonacciIterator::new();
    for num in fib_iter.take(15) {
        print!("{} ", num);
    }
    println!("\n");

    // 性能比较（较小的数值）
    println!("计算第 30 个斐波那契数:");

    let n = 30;

    // 递归方式（注意：较大的 n 会很慢）
    let result_recursive = fibonacci_recursive(n);
    println!("  递归方式: {}", result_recursive);

    // 迭代方式
    let result_iterative = fibonacci_iterative(n);
    println!("  迭代方式: {}", result_iterative);

    // 记忆化递归
    let result_memoized = fibonacci_memoized(n);
    println!("  记忆化递归: {}", result_memoized);

    // 尾递归
    let result_tail = fibonacci_tail_recursive(n);
    println!("  尾递归: {}", result_tail);

    println!("\n计算第 50 个斐波那契数（仅使用高效方法）:");
    let n_large = 50;
    println!("  迭代方式: {}", fibonacci_iterative(n_large));
    println!("  尾递归: {}", fibonacci_tail_recursive(n_large));

    // 展示迭代器的灵活性
    println!("\n找出前 10 个偶数的斐波那契数:");
    let even_fibs: Vec<u64> = FibonacciIterator::new()
        .filter(|&x| x % 2 == 0)
        .take(10)
        .collect();
    println!("{:?}", even_fibs);

    println!("\n前 10 个大于 1000 的斐波那契数:");
    let large_fibs: Vec<u64> = FibonacciIterator::new()
        .filter(|&x| x > 1000)
        .take(10)
        .collect();
    println!("{:?}", large_fibs);
}

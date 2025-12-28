# Config.toml 加载位置优先级

本文档说明 `memex_cli` 项目中 `config.toml` 配置文件的加载优先级、环境变量覆盖机制，以及数据目录的自动管理。

## 配置加载流程

配置加载由 `core/src/config/load.rs` 中的 `load_default()` 函数实现。加载流程如下：

### 1. 配置文件加载

配置系统按以下顺序查找并加载 `config.toml` 文件：

```rust
// Priority 1: ~/.memex/config.toml (highest)
let memex_config = get_memex_data_dir()?.join("config.toml");

// Priority 2: ./config.toml (current directory)
let local_config = Path::new("config.toml");

if memex_config.exists() {
    // Load from ~/.memex/config.toml
} else if local_config.exists() {
    // Load from ./config.toml
} else {
    // Use default config
}
```

**优先级规则：**

1. **用户数据目录下的 `config.toml`** - 最高优先级
   - 路径：`~/.memex/config.toml`（Windows: `%USERPROFILE%\.memex\config.toml`）
   - 用作全局配置，适用于所有项目
   - `~/.memex` 目录作为数据目录，用于存储配置和运行数据

2. **当前工作目录下的 `config.toml`** - 次优先级
   - 路径：`./config.toml`（相对于程序运行的当前目录）
   - 用作项目特定配置，覆盖全局配置
   - 如果 `~/.memex/config.toml` 不存在，且当前目录存在此文件，将被加载

3. **内置默认配置** - 最低优先级
   - 如果以上两个位置都不存在 `config.toml`，则使用代码中定义的默认值
   - 见 `core/src/config/types.rs` 中的 `AppConfig::default()`

### 2. 环境变量覆盖

配置文件加载完成后，以下环境变量会**覆盖**配置文件中的对应字段：

#### 2.1 后端类型配置

| 环境变量 | 覆盖字段 | 说明 |
|---------|---------|------|
| `MEM_CODECLI_BACKEND_KIND` | `backend_kind` | 指定后端类型（如 "codecli"） |

#### 2.2 Memory 服务配置

| 环境变量 | 覆盖字段 | 说明 |
|---------|---------|------|
| `MEM_CODECLI_MEMORY_URL` | `memory.provider.service.base_url` | Memory 服务的 URL |
| `MEM_CODECLI_MEMORY_API_KEY` | `memory.provider.service.api_key` | Memory 服务的 API 密钥 |

**注意：** 环境变量只有在值非空时才会覆盖配置文件中的值。

## 数据目录说明

`~/.memex` 目录作为全局数据目录，用于存储：

- **配置文件**：`~/.memex/config.toml`
- **日志文件**：`~/.memex/logs/memex-cli.<pid>.log`
- **事件输出**：`~/.memex/events_out/run.events.jsonl`
- **其他运行数据**

### 自动路径处理

系统会自动处理日志和事件输出路径，考虑了操作系统差异（Windows/Linux/macOS）：

#### 日志目录
- 如果配置中的 `logging.directory` 未设置或为空，系统会自动将其设置为 `~/.memex/logs`
- 如果显式指定了日志目录，则使用指定的路径
- 日志文件命名格式：`memex-cli.<进程ID>.log`
- 目录会在程序启动时自动创建

#### 事件输出目录
- 如果配置中的 `events_out.path` 为默认值 `./run.events.jsonl`，系统会自动将其重定向到 `~/.memex/events_out/run.events.jsonl`
- 如果显式指定了 events_out 路径，则使用指定的路径
- 目录会在程序启动时自动创建

## 完整优先级层次

从高到低排列：

```
┌─────────────────────────────────────────┐
│  1. 环境变量 (最高优先级)                 │
│     - MEM_CODECLI_BACKEND_KIND          │
│     - MEM_CODECLI_MEMORY_URL            │
│     - MEM_CODECLI_MEMORY_API_KEY        │
├─────────────────────────────────────────┤
│  2. 用户数据目录的 config.toml           │
│     - ~/.memex/config.toml              │
│     - Windows: %USERPROFILE%\.memex\... │
├─────────────────────────────────────────┤
│  3. 当前工作目录的 config.toml           │
│     - ./config.toml                     │
├─────────────────────────────────────────┤
│  4. 代码内置默认值 (最低优先级)          │
│     - AppConfig::default()              │
└─────────────────────────────────────────┘
```

## 配置示例

### 示例 1：使用全局配置

```bash
# 在用户数据目录创建全局配置
mkdir -p ~/.memex
cat > ~/.memex/config.toml << EOF
[memory]
provider = "service"
base_url = "https://memory.example.com"
api_key = "your-api-key"
EOF

# 在任何目录下运行，都会使用全局配置
cd /path/to/any/project
memex-cli run -- your-command
```

程序将：
1. 加载 `~/.memex/config.toml` 中的全局配置
2. 日志输出到 `~/.memex/logs/memex-cli.<pid>.log`
3. 事件输出到 `~/.memex/events_out/run.events.jsonl`

### 示例 2：项目特定配置覆盖全局配置

```bash
# 项目目录下创建项目特定配置
cd /path/to/project
cat > config.toml << EOF
[memory]
base_url = "https://dev-memory.example.com"
EOF

memex-cli run -- your-command
```

程序将：
1. 首先检查 `~/.memex/config.toml`（如果存在）
2. 如果不存在，则加载 `./config.toml` 中的项目配置

### 示例 3：环境变量覆盖配置文件

```bash
# 使用环境变量覆盖配置文件中的设置
export MEM_CODECLI_MEMORY_URL="https://prod-memory.example.com"
export MEM_CODECLI_MEMORY_API_KEY="prod-secret-key"
memex-cli run -- your-command
```

程序将：
1. 加载配置文件（优先 `~/.memex/config.toml`，其次 `./config.toml`）
2. 用环境变量覆盖 memory 相关配置（优先级最高）

### 示例 4：完全使用默认配置

```bash
# 在没有任何 config.toml 的情况下运行
# 确保 ~/.memex/config.toml 和 ./config.toml 都不存在
memex-cli run -- your-command
```

程序将：
1. 使用所有内置默认值
2. 日志自动输出到 `~/.memex/logs/memex-cli.<pid>.log`（自动创建目录）
3. 事件输出到 `~/.memex/events_out/run.events.jsonl`（自动创建目录）

### 示例 5：使用启动脚本

项目提供了 `start_with_env.ps1` PowerShell 脚本，用于设置环境变量后启动程序：

```powershell
# start_with_env.ps1 会设置必要的环境变量
.\start_with_env.ps1
```

## 配置查找路径说明

配置系统按以下顺序查找 `config.toml`：

1. ✅ **用户数据目录**：`~/.memex/config.toml`（Linux/macOS）或 `%USERPROFILE%\.memex\config.toml`（Windows）
2. ✅ **当前工作目录**：`./config.toml`
3. ✅ **内置默认值**：代码中的默认配置

**不会搜索的位置：**

- ❌ 系统配置目录（`/etc/memex/config.toml`）
- ❌ XDG 配置目录（`~/.config/memex/config.toml`）
- ❌ 可执行文件所在目录
- ❌ 父目录或子目录

### 跨平台路径说明

| 平台 | 用户数据目录路径 |
|------|----------------|
| Linux/macOS | `~/.memex/` |
| Windows | `%USERPROFILE%\.memex\` (例如 `C:\Users\YourName\.memex\`) |

## 最佳实践

1. **全局配置（推荐）**
   - 在 `~/.memex/config.toml` 中设置通用配置，适用于所有项目
   - 适合存储用户级别的设置（如 memory 服务 URL、日志级别等）
   - 所有运行数据统一存储在 `~/.memex` 目录下，便于管理

2. **项目特定配置**
   - 在项目根目录放置 `config.toml`，仅当需要项目特定设置时使用
   - 适合存储项目相关的配置差异（如开发/测试环境的不同设置）
   - 仅在 `~/.memex/config.toml` 不存在时才会被使用

3. **敏感信息处理**
   - 将敏感信息（如 API keys）通过环境变量传递，不写入配置文件
   - 环境变量具有最高优先级，会覆盖所有配置文件中的值
   - 在 CI/CD 或容器环境中优先使用环境变量

4. **版本控制**
   - 全局配置 `~/.memex/config.toml` 不应提交到版本控制
   - 提交示例配置文件（如 `config.toml.example`）到项目仓库
   - 将实际的项目 `config.toml` 添加到 `.gitignore`（如果包含敏感信息）

5. **数据目录管理**
   - `~/.memex/logs/` 目录会自动创建，用于存储运行日志
   - `~/.memex/events_out/` 目录会自动创建，用于存储事件日志
   - 定期清理旧的日志和事件文件以节省磁盘空间
   - 可以在配置中显式指定其他路径覆盖默认行为

6. **跨平台兼容性**
   - 所有路径处理都考虑了操作系统差异（Windows/Linux/macOS）
   - 使用 `PathBuf` 和 `to_string_lossy()` 确保路径在不同系统上正确工作
   - Windows 下支持 `%USERPROFILE%` 环境变量定位用户目录

## 技术实现细节

### 用户目录检测

系统通过以下环境变量检测用户主目录：
- Linux/macOS：`$HOME`
- Windows：`$USERPROFILE`（如果 `$HOME` 不存在）

### 日志目录处理

如果配置中的 `logging.directory` 未设置或为空字符串，系统会自动：
1. 将目录设置为 `~/.memex/logs`
2. 创建 `~/.memex/logs/` 目录（如果不存在）
3. 日志文件按进程 ID 命名：`memex-cli.<pid>.log`

如果配置中显式指定了其他目录，则使用指定的目录，不会进行重定向。

### events_out 路径处理

如果配置中的 `events_out.path` 为默认值 `./run.events.jsonl`，系统会自动：
1. 将路径重定向到 `~/.memex/events_out/run.events.jsonl`
2. 创建 `~/.memex/events_out/` 目录（如果不存在）

如果配置中显式指定了其他路径，则使用指定的路径，不会进行重定向。

## 相关文件

- 配置加载逻辑：[core/src/config/load.rs](../core/src/config/load.rs)
- 配置类型定义：[core/src/config/types.rs](../core/src/config/types.rs)
- 配置模式文档：[config-schema.md](./config-schema.md)
- 示例配置文件：[config.toml](../config.toml)
- 启动脚本：[start_with_env.ps1](../start_with_env.ps1)

## 未来改进建议

考虑增加以下功能：

1. **配置文件合并**：支持从 `~/.memex/config.toml` 和 `./config.toml` 合并配置，而非简单覆盖
2. **命令行参数**：支持通过 `--config` 参数指定配置文件路径
3. **配置验证**：在加载时验证配置的完整性和有效性，提供更友好的错误信息
4. **XDG 支持**：在 Linux 上支持 XDG Base Directory 规范（`$XDG_CONFIG_HOME/memex/config.toml`）
5. **日志轮转**：自动清理旧日志文件，或支持日志轮转策略

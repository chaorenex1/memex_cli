//! Configuration initialization wizard
use crate::commands::cli::InitArgs;
use memex_core::api as core_api;

/// Handle init command
pub async fn handle_init(
    args: InitArgs,
    _ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let memex_dir = core_api::get_memex_data_dir()?;
    let config_path = memex_dir.join("config.toml");

    // Check if config already exists
    if config_path.exists() {
        println!(
            "Configuration file already exists at: {}",
            config_path.display()
        );
        println!("To reconfigure, please edit the file directly or delete it and run init again.");
        return Ok(());
    }

    println!("### Memex Configuration Wizard\n");
    println!(
        "This will create a configuration file at: {}",
        config_path.display()
    );
    println!();

    // Determine memory provider
    let provider = if args.non_interactive {
        args.provider.clone()
    } else {
        prompt_provider()?
    };

    // Generate configuration based on provider
    let config_content = match provider.as_str() {
        "local" => generate_local_config(&args)?,
        "hybrid" => generate_hybrid_config(&args)?,
        "service" => generate_service_config(&args)?,
        _ => {
            return Err(core_api::CliError::Command(format!(
                "Unknown provider: {}. Use 'local', 'hybrid', or 'service'.",
                provider
            )));
        }
    };

    // Create memex directory
    std::fs::create_dir_all(&memex_dir).map_err(|e| {
        core_api::CliError::Command(format!("Failed to create memex directory: {}", e))
    })?;

    // Write configuration
    std::fs::write(&config_path, config_content).map_err(|e| {
        core_api::CliError::Command(format!("Failed to write configuration: {}", e))
    })?;

    println!();
    println!("### Configuration Created Successfully\n");
    println!("File: {}", config_path.display());
    println!("Provider: {}", provider);
    println!();
    println!("You can now run memex commands. For example:");
    println!("  memex init                    # Show this help");
    println!("  memex db init                 # Initialize the local database");
    println!("  memex db info                 # Show database information");
    println!("  memex search --query \"help\"   # Search memory");
    println!();

    Ok(())
}

fn prompt_provider() -> Result<String, core_api::CliError> {
    println!("Select memory provider:");
    println!("  1. local    - Local-only storage with LanceDB");
    println!("  2. hybrid   - Local storage with remote sync");
    println!("  3. service  - Remote service only");
    println!();

    print!("Enter choice (1-3) [default: 1]: ");
    use std::io::Write;
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| core_api::CliError::Command(format!("Failed to read input: {}", e)))?;

    match input.trim() {
        "2" | "hybrid" => Ok("hybrid".to_string()),
        "3" | "service" => Ok("service".to_string()),
        "" | "1" | "local" => Ok("local".to_string()),
        _ => Err(core_api::CliError::Command("Invalid choice".to_string())),
    }
}

fn prompt_embedding_provider() -> Result<String, core_api::CliError> {
    println!();
    println!("Select embedding provider:");
    println!("  1. ollama   - Local Ollama (recommended for privacy)");
    println!("  2. openai   - OpenAI API");
    println!();

    print!("Enter choice (1-2) [default: 1]: ");
    use std::io::Write;
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| core_api::CliError::Command(format!("Failed to read input: {}", e)))?;

    match input.trim() {
        "2" | "openai" => Ok("openai".to_string()),
        "" | "1" | "ollama" => Ok("ollama".to_string()),
        _ => Err(core_api::CliError::Command("Invalid choice".to_string())),
    }
}

fn generate_local_config(args: &InitArgs) -> Result<String, core_api::CliError> {
    let embedding_provider = if args.non_interactive {
        if args.openai_key.is_some() {
            "openai".to_string()
        } else {
            "ollama".to_string()
        }
    } else {
        prompt_embedding_provider()?
    };

    let config = match embedding_provider.as_str() {
        "openai" => {
            let api_key = args.openai_key.clone().unwrap_or_else(|| {
                print!("Enter OpenAI API key: ");
                use std::io::Write;
                std::io::stdout().flush().unwrap();
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).unwrap();
                input.trim().to_string()
            });

            format!(
                r#"[memory]
enabled = true
provider = {{ local = {{}}

[memory.local]
db_path = "~/.memex/db"
search_limit = 10
min_score = 0.6

[memory.local.embedding]
provider = "openai"

[memory.local.embedding.openai]
base_url = "https://api.openai.com/v1"
api_key = "{}"
model = "text-embedding-3-small"

[memory.local.sync]
enabled = false
interval_secs = 300
batch_size = 50
max_retries = 3
conflict_resolution = "last_write_wins"
"#,
                api_key
            )
        }
        _ => {
            format!(
                r#"[memory]
enabled = true
provider = {{ local = {{}}

[memory.local]
db_path = "~/.memex/db"
search_limit = 10
min_score = 0.6

[memory.local.embedding]
provider = "ollama"

[memory.local.embedding.ollama]
base_url = "{}"
model = "nomic-embed-text"
dimension = 768

[memory.local.sync]
enabled = false
interval_secs = 300
batch_size = 50
max_retries = 3
conflict_resolution = "last_write_wins"
"#,
                args.ollama_url
            )
        }
    };

    Ok(config)
}

fn generate_hybrid_config(args: &InitArgs) -> Result<String, core_api::CliError> {
    let embedding_provider = if args.non_interactive {
        if args.openai_key.is_some() {
            "openai".to_string()
        } else {
            "ollama".to_string()
        }
    } else {
        prompt_embedding_provider()?
    };

    let (remote_url, remote_key) = if args.non_interactive {
        (
            args.remote_url
                .clone()
                .unwrap_or_else(|| "http://localhost:8080".to_string()),
            args.remote_key.clone().unwrap_or_default(),
        )
    } else {
        print!("Enter remote memory service URL [default: http://localhost:8080]: ");
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let url = input.trim().to_string();
        let url = if url.is_empty() {
            "http://localhost:8080".to_string()
        } else {
            url
        };

        print!("Enter remote API key (optional): ");
        std::io::stdout().flush().unwrap();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        (url, input.trim().to_string())
    };

    let config = match embedding_provider.as_str() {
        "openai" => {
            let api_key = args.openai_key.clone().unwrap_or_else(|| {
                if !args.non_interactive {
                    print!("Enter OpenAI API key: ");
                    use std::io::Write;
                    std::io::stdout().flush().unwrap();
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).unwrap();
                    input.trim().to_string()
                } else {
                    String::new()
                }
            });

            format!(
                r#"[memory]
enabled = true
provider = {{ hybrid = {{}}

[memory.hybrid.local]
db_path = "~/.memex/db"
search_limit = 10
min_score = 0.6

[memory.hybrid.local.embedding]
provider = "openai"

[memory.hybrid.local.embedding.openai]
base_url = "https://api.openai.com/v1"
api_key = "{}"
model = "text-embedding-3-small"

[memory.hybrid.local.sync]
enabled = true
interval_secs = 300
batch_size = 50
max_retries = 3
conflict_resolution = "last_write_wins"

[memory.hybrid.remote]
base_url = "{}"
api_key = "{}"
timeout_ms = 30000

[memory.hybrid.sync_strategy]
type = "LocalFirst"
"#,
                api_key, remote_url, remote_key
            )
        }
        _ => {
            format!(
                r#"[memory]
enabled = true
provider = {{ hybrid = {{}}

[memory.hybrid.local]
db_path = "~/.memex/db"
search_limit = 10
min_score = 0.6

[memory.hybrid.local.embedding]
provider = "ollama"

[memory.hybrid.local.embedding.ollama]
base_url = "{}"
model = "nomic-embed-text"
dimension = 768

[memory.hybrid.local.sync]
enabled = true
interval_secs = 300
batch_size = 50
max_retries = 3
conflict_resolution = "last_write_wins"

[memory.hybrid.remote]
base_url = "{}"
api_key = "{}"
timeout_ms = 30000

[memory.hybrid.sync_strategy]
type = "LocalFirst"
"#,
                args.ollama_url, remote_url, remote_key
            )
        }
    };

    Ok(config)
}

fn generate_service_config(args: &InitArgs) -> Result<String, core_api::CliError> {
    let (base_url, api_key) = if args.non_interactive {
        (
            args.remote_url
                .clone()
                .unwrap_or_else(|| "http://localhost:8080".to_string()),
            args.remote_key.clone().unwrap_or_default(),
        )
    } else {
        print!("Enter memory service URL [default: http://localhost:8080]: ");
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let url = input.trim().to_string();
        let url = if url.is_empty() {
            "http://localhost:8080".to_string()
        } else {
            url
        };

        print!("Enter API key (optional): ");
        std::io::stdout().flush().unwrap();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        (url, input.trim().to_string())
    };

    let config = format!(
        r#"[memory]
enabled = true
provider = {{ service = {{}}

[memory.service]
base_url = "{}"
api_key = "{}"
timeout_ms = 30000
"#,
        base_url, api_key
    );

    Ok(config)
}

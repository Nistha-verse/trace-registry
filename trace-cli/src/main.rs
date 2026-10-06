use {
    serde_json::json,
    std::{
        env,
        io::{Read, Write},
        net::TcpListener,
    },
    trace_cli::{append_history, ClientError, TraceClient, DEFAULT_TRACE_PROGRAM_ID},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("trace: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), ClientError> {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("check");
    let rpc_url = option(&args, "--rpc-url").unwrap_or_else(|| {
        env::var("TRACE_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8899".into())
    });
    let trace_program = option(&args, "--trace-program").unwrap_or_else(|| {
        env::var("TRACE_PROGRAM_ID").unwrap_or_else(|_| DEFAULT_TRACE_PROGRAM_ID.into())
    });
    let client = TraceClient::new(rpc_url, &trace_program)?;

    match command {
        "check" => {
            let target = positional(&args, 1)?;
            let report = client.verify(target)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "history" => {
            let target = positional(&args, 1)?;
            let limit = option(&args, "--limit")
                .unwrap_or_else(|| "100".into())
                .parse::<usize>()
                .map_err(|_| ClientError::Rpc("--limit must be an integer".into()))?;
            let records = client.history(target, limit)?;
            if let Some(path) = option(&args, "--file") {
                append_history(path, &records)?;
            }
            println!("{}", serde_json::to_string_pretty(&records)?);
        }
        "serve" => serve(
            client,
            option(&args, "--bind").unwrap_or_else(|| "127.0.0.1:8787".into()),
        )?,
        "help" | "--help" | "-h" => print_help(),
        other => return Err(ClientError::Rpc(format!("unknown command `{other}`"))),
    }
    Ok(())
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn positional(args: &[String], index: usize) -> Result<&str, ClientError> {
    args.iter()
        .filter(|arg| !arg.starts_with("--"))
        .nth(index)
        .map(String::as_str)
        .ok_or_else(|| ClientError::Rpc("usage: trace-cli check PROGRAM [--rpc-url URL]".into()))
}

fn serve(client: TraceClient, bind: String) -> Result<(), ClientError> {
    let listener = TcpListener::bind(&bind)?;
    eprintln!("trace verifier listening on http://{bind}");
    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut request = [0u8; 4096];
        let size = stream.read(&mut request)?;
        let request = String::from_utf8_lossy(&request[..size]);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let (status, body) = if path == "/health" {
            ("200 OK", json!({"ok": true}))
        } else if let Some(program) = path.strip_prefix("/v1/programs/") {
            let (program, history) = program
                .strip_suffix("/history")
                .map_or((program, false), |program| (program, true));
            let response = if history {
                client.history(program, 100).map(|records| json!(records))
            } else {
                client.verify(program).map(|report| json!(report))
            };
            match response {
                Ok(report) => ("200 OK", report),
                Err(error) => ("400 Bad Request", json!({"error": error.to_string()})),
            }
        } else {
            ("404 Not Found", json!({"error": "not found"}))
        };
        let body = serde_json::to_string(&body)?;
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )?;
    }
    Ok(())
}

fn print_help() {
    println!(
        "Trace public verifier\n\n  trace-cli check PROGRAM [--rpc-url URL] [--trace-program ID]\n  trace-cli history PROGRAM [--limit N] [--file PATH]\n  trace-cli serve [--bind HOST:PORT] [--rpc-url URL] [--trace-program ID]\n\nEnvironment: TRACE_RPC_URL and TRACE_PROGRAM_ID"
    );
}

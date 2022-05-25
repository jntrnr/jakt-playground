use std::{env, fs, process::Stdio};

use async_std::process::Command;
use compiler::compile;
use tide::{
    http::headers::HeaderValue,
    prelude::*,
    security::{CorsMiddleware, Origin},
    Body, Request, Response,
};
use tide_governor::GovernorMiddleware;

mod compiler;

macro_rules! env_get {
    ($name: expr, $default: expr) => {
        std::env::var($name).unwrap_or_else(|_| {
            eprintln!("Warning: {} is not set, using default {}", $name, $default);
            String::from($default)
        })
    };
}

#[derive(Deserialize)]
#[serde(default)]
struct CompileRequest {
    execute: bool,
}

impl Default for CompileRequest {
    fn default() -> Self {
        Self { execute: false }
    }
}

async fn compile_or_execute(mut req: Request<()>) -> tide::Result {
    let code = req.body_string().await?;
    let params: CompileRequest = req.query()?;
    match compile(&code, params.execute) {
        Ok(result) => {
            let mut response = Response::new(200);
            response.set_body(Body::from_json(&result)?);
            Ok(response)
        }
        Err(_) => Ok(Response::new(500)),
    }
}

fn program_exists(program: &str) -> bool {
    if let Ok(paths) = env::var("PATH") {
        for path in paths.split(":") {
            if fs::metadata(format!("{}/{}", path, program)).is_ok() {
                return true;
            }
        }
    }
    false
}

async fn has_docker_image() -> bool {
    if let Ok(child) = Command::new("docker")
        .arg("images")
        .stdout(Stdio::piped())
        .spawn()
    {
        if let Ok(output) = child.output().await {
            if let Ok(string) = String::from_utf8(output.stdout) {
                return string.contains("jakt_sandbox");
            }
        }
    }
    false
}

#[async_std::main]
async fn main() -> tide::Result<()> {
    if !program_exists("jakt") {
        eprintln!("Jakt not found. Have you ran 'cargo install --path . inside jakt repository?'");
        return Ok(());
    }

    if !program_exists("docker") {
        eprintln!("Docker not found. Please install it: https://docs.docker.com/get-docker/");
        return Ok(());
    }

    if !has_docker_image().await {
        eprintln!("Docker image jakt_sandbox is missing. Have you ran 'sh ./sandbox/setup.sh'?");
        return Ok(());
    }

    let mut app = tide::new();
    let host = format!("127.0.0.1:{}", env_get!("PORT", "8080"));
    app.with(
        CorsMiddleware::new()
            .allow_methods("POST".parse::<HeaderValue>().unwrap())
            .allow_origin(Origin::from(env_get!("ALLOW_ORIGIN", "*")))
            .allow_credentials(false),
    );
    app.at("/compile")
        .with(GovernorMiddleware::per_minute(4)?)
        .post(compile_or_execute);
    println!("Listening to {}", host);
    app.listen(host).await?;
    Ok(())
}

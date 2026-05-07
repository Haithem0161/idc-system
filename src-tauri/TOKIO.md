# Tokio Guide

Event-driven, non-blocking async runtime for building reliable network applications in Rust.

## Installation

Add Tokio to your `Cargo.toml` with the full feature set:

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
```

For fine-grained control, enable only the features you need:

```toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "sync", "signal", "time"] }
```

Feature flags reference:

| Feature            | Provides                                      |
|--------------------|-----------------------------------------------|
| `rt`               | Core single-threaded runtime                  |
| `rt-multi-thread`  | Multi-threaded work-stealing scheduler        |
| `macros`           | `#[tokio::main]` and `#[tokio::test]` macros  |
| `net`              | TCP, UDP, Unix socket I/O                     |
| `io-util`          | `AsyncReadExt`, `AsyncWriteExt` helpers       |
| `sync`             | Channels (`mpsc`, `broadcast`, `oneshot`, `watch`) and sync primitives |
| `signal`           | OS signal handling (`ctrl_c`, `SIGTERM`)       |
| `time`             | `sleep`, `timeout`, `interval`                |
| `fs`               | Async filesystem operations                   |
| `full`             | All features enabled                          |

## Basic Usage

A minimal async application using the `#[tokio::main]` attribute macro:

```rust
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    println!("Starting application...");
    sleep(Duration::from_secs(1)).await;
    println!("Application ready.");
}
```

Running two concurrent tasks:

```rust
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let handle_a = tokio::spawn(async {
        sleep(Duration::from_millis(500)).await;
        "Task A complete"
    });

    let handle_b = tokio::spawn(async {
        sleep(Duration::from_millis(300)).await;
        "Task B complete"
    });

    let (result_a, result_b) = tokio::join!(handle_a, handle_b);
    println!("{}", result_a.unwrap());
    println!("{}", result_b.unwrap());
}
```

## Runtime

### #[tokio::main] Attribute Macro

The most common way to bootstrap the Tokio runtime. Transforms an `async fn main()` into a synchronous entry point that starts the runtime:

```rust
// Default multi-threaded runtime (uses all available CPU cores)
#[tokio::main]
async fn main() {
    println!("Running on the multi-threaded runtime");
}

// Equivalent expanded form:
fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            println!("Running on the multi-threaded runtime");
        });
}
```

### Single-Threaded Runtime

Use the `current_thread` flavor for lightweight applications or when `Send` bounds are problematic:

```rust
#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("Running on a single-threaded runtime");
}
```

### Custom Runtime with Builder

For enterprise applications requiring fine-tuned configuration:

```rust
use tokio::runtime::Runtime;

fn main() {
    let runtime = Runtime::new().unwrap();

    runtime.block_on(async {
        println!("Running inside block_on");
    });
}
```

### Advanced Runtime Builder

```rust
use tokio::runtime::Builder;

fn main() {
    let runtime = Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("my-app-worker")
        .thread_stack_size(3 * 1024 * 1024) // 3 MB
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        // Application entry point
        run_server().await;
    });
}

async fn run_server() {
    println!("Server started with custom runtime configuration");
}
```

### #[tokio::test] for Async Tests

```rust
#[cfg(test)]
mod tests {
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_async_operation() {
        sleep(Duration::from_millis(10)).await;
        assert_eq!(2 + 2, 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_with_multi_thread() {
        let handle = tokio::spawn(async { 42 });
        assert_eq!(handle.await.unwrap(), 42);
    }
}
```

## Task Spawning

### tokio::spawn

Spawns a new asynchronous task that runs concurrently. The provided future starts executing immediately in the background. Returns a `JoinHandle` for awaiting the result:

```rust
use std::io;
use tokio::net::{TcpListener, TcpStream};

async fn process(socket: TcpStream) {
    // Handle the connection
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from: {}", addr);

        // Each connection is processed in its own task
        tokio::spawn(async move {
            process(socket).await;
        });
    }
}
```

### JoinHandle

The `JoinHandle` returned by `tokio::spawn` is itself a future. Awaiting it yields the task's return value or a `JoinError` if the task panicked:

```rust
use tokio::task::JoinHandle;

#[tokio::main]
async fn main() {
    let handle: JoinHandle<i32> = tokio::spawn(async {
        // Perform some computation
        5 + 3
    });

    match handle.await {
        Ok(result) => println!("Task returned: {}", result),
        Err(e) => eprintln!("Task failed: {}", e),
    }
}
```

### Spawning Multiple Tasks and Collecting Results

```rust
async fn my_background_op(id: i32) -> String {
    format!("Result from task {}", id)
}

#[tokio::main]
async fn main() {
    let ops = vec![1, 2, 3, 4, 5];
    let mut tasks = Vec::with_capacity(ops.len());

    for op in ops {
        tasks.push(tokio::spawn(my_background_op(op)));
    }

    let mut outputs = Vec::with_capacity(tasks.len());
    for task in tasks {
        outputs.push(task.await.unwrap());
    }

    println!("{:?}", outputs);
}
```

### JoinSet

`JoinSet` manages a collection of spawned tasks and allows awaiting their completion. Preferred over manually tracking `Vec<JoinHandle>`:

```rust
use tokio::task::JoinSet;

#[tokio::main]
async fn main() {
    let mut set = JoinSet::new();

    for i in 0..10 {
        set.spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100 * i)).await;
            format!("Task {} done", i)
        });
    }

    // Await tasks as they complete (not necessarily in order)
    while let Some(result) = set.join_next().await {
        match result {
            Ok(value) => println!("{}", value),
            Err(e) => eprintln!("Task failed: {}", e),
        }
    }
}
```

### JoinSet with Abort

```rust
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let mut set = JoinSet::new();

    for i in 0..5 {
        set.spawn(async move {
            sleep(Duration::from_secs(i)).await;
            i
        });
    }

    // Wait for the first task to complete
    if let Some(result) = set.join_next().await {
        println!("First completed: {:?}", result.unwrap());
    }

    // Abort all remaining tasks
    set.abort_all();

    // Drain remaining results (will be JoinError with cancelled)
    while let Some(result) = set.join_next().await {
        match result {
            Ok(val) => println!("Completed before abort: {}", val),
            Err(_) => println!("Task was aborted"),
        }
    }
}
```

### spawn_blocking

Offload CPU-intensive or synchronous blocking work to a dedicated thread pool so the async runtime is not starved:

```rust
use tokio::task;

#[tokio::main]
async fn main() {
    // Spawn a blocking operation on the blocking thread pool
    let result = task::spawn_blocking(|| {
        // Simulate expensive computation
        let mut sum: u64 = 0;
        for i in 0..1_000_000 {
            sum += i;
        }
        sum
    })
    .await
    .unwrap();

    println!("Computation result: {}", result);
}
```

### spawn_blocking with Async Interop

```rust
use tokio::task;
use std::time::Instant;

#[tokio::main]
async fn main() {
    let start = Instant::now();

    // Run blocking I/O (e.g., reading a large file synchronously)
    let content = task::spawn_blocking(|| {
        std::fs::read_to_string("/etc/hostname").unwrap_or_default()
    })
    .await
    .unwrap();

    println!("File content: {} (read in {:?})", content.trim(), start.elapsed());
}
```

## Channels

### mpsc (Multi-Producer, Single-Consumer)

The most commonly used channel. Multiple senders can transmit messages to a single receiver. Bounded channels provide backpressure:

```rust
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    // Create a bounded channel with capacity of 32
    let (tx, mut rx) = mpsc::channel::<String>(32);

    // Spawn multiple producer tasks
    for i in 0..5 {
        let tx = tx.clone();
        tokio::spawn(async move {
            let msg = format!("Message from producer {}", i);
            tx.send(msg).await.unwrap();
        });
    }

    // Drop the original sender so the receiver knows when all senders are gone
    drop(tx);

    // Receive all messages
    while let Some(msg) = rx.recv().await {
        println!("Received: {}", msg);
    }

    println!("All producers finished.");
}
```

### mpsc with Send Timeout

```rust
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel(1);

    tokio::spawn(async move {
        for i in 0..10 {
            if let Err(e) = tx.send_timeout(i, Duration::from_millis(100)).await {
                println!("Send error: {:?}", e);
                return;
            }
        }
    });

    while let Some(i) = rx.recv().await {
        println!("Got = {}", i);
        sleep(Duration::from_millis(200)).await;
    }
}
```

### mpsc for Funneling Writes to a Single Socket

```rust
use tokio::io::{self, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut socket = TcpStream::connect("127.0.0.1:8080").await?;
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(100);

    for i in 0..10 {
        let tx = tx.clone();
        tokio::spawn(async move {
            let data = format!("data packet {}\n", i).into_bytes();
            tx.send(data).await.unwrap();
        });
    }

    // Drop original sender so rx.recv() returns None when all senders drop
    drop(tx);

    while let Some(data) = rx.recv().await {
        socket.write_all(&data).await?;
    }

    Ok(())
}
```

### broadcast (Multi-Producer, Multi-Consumer)

Every active receiver gets a copy of each sent message. Useful for fan-out scenarios like event notification:

```rust
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    let (tx, _) = broadcast::channel::<String>(16);

    // Subscribe multiple receivers
    let mut rx1 = tx.subscribe();
    let mut rx2 = tx.subscribe();

    tokio::spawn(async move {
        while let Ok(msg) = rx1.recv().await {
            println!("Receiver 1: {}", msg);
        }
    });

    tokio::spawn(async move {
        while let Ok(msg) = rx2.recv().await {
            println!("Receiver 2: {}", msg);
        }
    });

    // Send messages - all receivers get a copy
    tx.send("Event: user_login".to_string()).unwrap();
    tx.send("Event: data_update".to_string()).unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}
```

### broadcast for Shutdown Signal

```rust
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    for id in 0..3 {
        let mut shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            tokio::select! {
                _ = async {
                    loop {
                        sleep(Duration::from_millis(500)).await;
                        println!("Worker {} doing work...", id);
                    }
                } => {}
                _ = shutdown_rx.recv() => {
                    println!("Worker {} shutting down", id);
                }
            }
        });
    }

    // Let workers run for 2 seconds then signal shutdown
    sleep(Duration::from_secs(2)).await;
    println!("Sending shutdown signal...");
    let _ = shutdown_tx.send(());
    sleep(Duration::from_millis(100)).await;
}
```

### oneshot (Single Value, Single Use)

Sends exactly one value from a single producer to a single consumer. Ideal for request-response patterns:

```rust
use tokio::sync::oneshot;

async fn some_computation() -> String {
    "represents the result of the computation".to_string()
}

#[tokio::main]
async fn main() {
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let res = some_computation().await;
        tx.send(res).unwrap();
    });

    // Do other work while the computation is happening in the background

    // Wait for the computation result
    let res = rx.await.unwrap();
    println!("Received: {}", res);
}
```

### oneshot for Request-Response with mpsc

A common pattern combining `mpsc` and `oneshot` to create a command/response channel for managing shared resources:

```rust
use tokio::sync::{mpsc, oneshot};

enum Command {
    Get { key: String, resp: oneshot::Sender<Option<String>> },
    Set { key: String, value: String, resp: oneshot::Sender<()> },
}

#[tokio::main]
async fn main() {
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(32);

    // Spawn the state manager task
    tokio::spawn(async move {
        let mut store = std::collections::HashMap::new();

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                Command::Get { key, resp } => {
                    let value = store.get(&key).cloned();
                    let _ = resp.send(value);
                }
                Command::Set { key, value, resp } => {
                    store.insert(key, value);
                    let _ = resp.send(());
                }
            }
        }
    });

    // Set a value
    let (resp_tx, resp_rx) = oneshot::channel();
    cmd_tx.send(Command::Set {
        key: "name".to_string(),
        value: "Tokio".to_string(),
        resp: resp_tx,
    }).await.unwrap();
    resp_rx.await.unwrap();

    // Get a value
    let (resp_tx, resp_rx) = oneshot::channel();
    cmd_tx.send(Command::Get {
        key: "name".to_string(),
        resp: resp_tx,
    }).await.unwrap();
    let value = resp_rx.await.unwrap();
    println!("Got: {:?}", value); // Got: Some("Tokio")
}
```

### watch (Single Value, Multiple Observers)

A single-producer, multi-consumer channel where receivers see only the most recent value. Ideal for configuration updates and state broadcasting:

```rust
use tokio::sync::watch;
use tokio::time::{sleep, Duration};

#[derive(Debug, Clone)]
struct AppConfig {
    max_connections: usize,
    timeout_secs: u64,
}

#[tokio::main]
async fn main() {
    let initial_config = AppConfig {
        max_connections: 100,
        timeout_secs: 30,
    };

    let (config_tx, config_rx) = watch::channel(initial_config);

    // Spawn worker tasks that observe config changes
    for id in 0..3 {
        let mut rx = config_rx.clone();
        tokio::spawn(async move {
            loop {
                // Wait for the config to change
                if rx.changed().await.is_err() {
                    break; // Sender dropped
                }
                let config = rx.borrow_and_update().clone();
                println!("Worker {} sees new config: {:?}", id, config);
            }
        });
    }

    // Simulate config updates
    sleep(Duration::from_secs(1)).await;
    config_tx.send(AppConfig {
        max_connections: 200,
        timeout_secs: 60,
    }).unwrap();

    sleep(Duration::from_secs(1)).await;
    config_tx.send(AppConfig {
        max_connections: 50,
        timeout_secs: 15,
    }).unwrap();

    sleep(Duration::from_millis(100)).await;
}
```

## Synchronization

### Mutex

Tokio's async-aware mutex. Use this when you need to hold a lock across `.await` points. For synchronous-only critical sections, prefer `std::sync::Mutex` as it is faster:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    let counter = Arc::new(Mutex::new(0u64));

    let mut handles = vec![];

    for _ in 0..10 {
        let counter = Arc::clone(&counter);
        handles.push(tokio::spawn(async move {
            let mut lock = counter.lock().await;
            *lock += 1;
            // Safe to hold across .await if needed
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    println!("Counter: {}", *counter.lock().await);
}
```

### RwLock

Allows multiple concurrent readers or a single exclusive writer. Use when reads significantly outnumber writes:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    let data = Arc::new(RwLock::new(vec![1, 2, 3]));

    let mut handles = vec![];

    // Spawn reader tasks
    for i in 0..5 {
        let data = Arc::clone(&data);
        handles.push(tokio::spawn(async move {
            let read_guard = data.read().await;
            println!("Reader {}: {:?}", i, *read_guard);
        }));
    }

    // Spawn a writer task
    {
        let data = Arc::clone(&data);
        handles.push(tokio::spawn(async move {
            let mut write_guard = data.write().await;
            write_guard.push(4);
            println!("Writer: appended 4, data = {:?}", *write_guard);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}
```

### Semaphore

Controls concurrent access to a limited resource. Use for connection pools, rate limiting, and bounding concurrency:

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    // Allow at most 3 concurrent operations
    let semaphore = Arc::new(Semaphore::new(3));

    let mut handles = vec![];

    for i in 0..10 {
        let semaphore = Arc::clone(&semaphore);
        handles.push(tokio::spawn(async move {
            // Acquire a permit before proceeding
            let _permit = semaphore.acquire().await.unwrap();
            println!("Task {} acquired permit", i);
            sleep(Duration::from_millis(500)).await;
            println!("Task {} releasing permit", i);
            // Permit is released when _permit is dropped
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}
```

### Semaphore with OwnedSemaphorePermit

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;

async fn process_with_permit(semaphore: Arc<Semaphore>, id: u32) {
    // OwnedSemaphorePermit can be moved across tasks
    let permit = semaphore.acquire_owned().await.unwrap();
    println!("Processing request {}", id);
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    drop(permit); // Explicitly release
}

#[tokio::main]
async fn main() {
    let semaphore = Arc::new(Semaphore::new(5));

    let mut handles = vec![];
    for i in 0..20 {
        let sem = Arc::clone(&semaphore);
        handles.push(tokio::spawn(process_with_permit(sem, i)));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}
```

### Notify

A lightweight notification primitive for signaling between tasks. Unlike channels, `Notify` does not carry data:

```rust
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let notify = Arc::new(Notify::new());

    let notify_clone = Arc::clone(&notify);
    let waiter = tokio::spawn(async move {
        println!("Waiting for notification...");
        notify_clone.notified().await;
        println!("Received notification!");
    });

    // Simulate some work
    sleep(Duration::from_secs(1)).await;
    println!("Sending notification...");
    notify.notify_one();

    waiter.await.unwrap();
}
```

### Notify for Producer-Consumer Coordination

```rust
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

struct SharedQueue {
    data: Mutex<Vec<String>>,
    notify: Notify,
}

impl SharedQueue {
    fn new() -> Self {
        Self {
            data: Mutex::new(Vec::new()),
            notify: Notify::new(),
        }
    }

    async fn push(&self, item: String) {
        self.data.lock().await.push(item);
        self.notify.notify_one();
    }

    async fn pop(&self) -> String {
        loop {
            // Check if there is data available
            {
                let mut data = self.data.lock().await;
                if let Some(item) = data.pop() {
                    return item;
                }
            }
            // Wait for a notification that data is available
            self.notify.notified().await;
        }
    }
}

#[tokio::main]
async fn main() {
    let queue = Arc::new(SharedQueue::new());

    let producer_queue = Arc::clone(&queue);
    tokio::spawn(async move {
        for i in 0..5 {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            producer_queue.push(format!("item-{}", i)).await;
        }
    });

    for _ in 0..5 {
        let item = queue.pop().await;
        println!("Consumed: {}", item);
    }
}
```

## TCP I/O

### TcpListener

Bind to an address and accept incoming TCP connections:

```rust
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Server listening on 127.0.0.1:8080");

    loop {
        let (mut socket, addr) = listener.accept().await?;
        println!("New connection from: {}", addr);

        tokio::spawn(async move {
            let mut buf = vec![0u8; 1024];

            loop {
                let n = match socket.read(&mut buf).await {
                    Ok(0) => return, // Connection closed
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("Read error: {}", e);
                        return;
                    }
                };

                // Echo back the received data
                if let Err(e) = socket.write_all(&buf[..n]).await {
                    eprintln!("Write error: {}", e);
                    return;
                }
            }
        });
    }
}
```

### TcpStream Client

```rust
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    println!("Connected to server");

    // Send data
    stream.write_all(b"Hello, server!").await?;

    // Read response
    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await?;
    println!("Received: {}", String::from_utf8_lossy(&buf[..n]));

    Ok(())
}
```

### Splitting a TcpStream

Split a stream into independent read and write halves for use in separate tasks:

```rust
use tokio::net::TcpStream;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = TcpStream::connect("127.0.0.1:8080").await?;
    let (reader, mut writer) = stream.into_split();

    let read_handle = tokio::spawn(async move {
        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            let n = buf_reader.read_line(&mut line).await.unwrap();
            if n == 0 {
                break;
            }
            println!("Received: {}", line.trim());
        }
    });

    let write_handle = tokio::spawn(async move {
        for i in 0..5 {
            let msg = format!("Message {}\n", i);
            writer.write_all(msg.as_bytes()).await.unwrap();
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    });

    let _ = tokio::join!(read_handle, write_handle);
    Ok(())
}
```

### Production TCP Server with Graceful Shutdown

```rust
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

async fn handle_connection(
    mut socket: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let mut buf = vec![0u8; 4096];

    loop {
        tokio::select! {
            result = socket.read(&mut buf) => {
                match result {
                    Ok(0) => {
                        println!("[{}] Connection closed", addr);
                        return;
                    }
                    Ok(n) => {
                        if socket.write_all(&buf[..n]).await.is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        eprintln!("[{}] Error: {}", addr, e);
                        return;
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                println!("[{}] Shutting down connection", addr);
                let _ = socket.write_all(b"Server shutting down\n").await;
                return;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    println!("Server listening on 127.0.0.1:8080");

    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        println!("Shutdown signal received");
        let _ = shutdown_tx_clone.send(());
    });

    let mut shutdown_rx = shutdown_tx.subscribe();

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (socket, addr) = result?;
                let shutdown_rx = shutdown_tx.subscribe();
                tokio::spawn(handle_connection(socket, addr, shutdown_rx));
            }
            _ = shutdown_rx.recv() => {
                println!("Server shutting down...");
                break;
            }
        }
    }

    // Give active connections time to finish
    sleep(Duration::from_secs(1)).await;
    Ok(())
}
```

## Signal Handling

### ctrl_c (Cross-Platform)

The simplest way to handle Ctrl+C. Works on both Unix and Windows:

```rust
use tokio::signal;

#[tokio::main]
async fn main() {
    println!("Application started. Press Ctrl+C to exit.");

    signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");

    println!("Ctrl+C received. Cleaning up...");
    // Perform cleanup here
}
```

### SignalKind on Unix (SIGTERM, SIGHUP, etc.)

Handle specific Unix signals for production daemon processes:

```rust
#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sighup = signal(SignalKind::hangup())?;

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("Received SIGINT (Ctrl+C)");
            }
            _ = sigterm.recv() => {
                println!("Received SIGTERM");
            }
            _ = sighup.recv() => {
                println!("Received SIGHUP - reloading configuration");
            }
        }
    }

    #[cfg(windows)]
    {
        tokio::signal::ctrl_c().await?;
        println!("Received Ctrl+C");
    }

    println!("Shutting down gracefully...");
    Ok(())
}
```

### Enterprise Graceful Shutdown Pattern

```rust
use tokio::sync::watch;
use tokio::time::{sleep, Duration};

async fn run_worker(id: u32, mut shutdown: watch::Receiver<bool>) {
    loop {
        tokio::select! {
            _ = async {
                // Simulate periodic work
                sleep(Duration::from_secs(1)).await;
                println!("Worker {} completed a unit of work", id);
            } => {}
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    println!("Worker {} received shutdown signal, finishing...", id);
                    // Perform per-worker cleanup
                    sleep(Duration::from_millis(100)).await;
                    println!("Worker {} shut down cleanly", id);
                    return;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let mut handles = vec![];
    for id in 0..4 {
        let rx = shutdown_rx.clone();
        handles.push(tokio::spawn(run_worker(id, rx)));
    }

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await.unwrap();
    println!("Shutdown initiated...");

    // Signal all workers to stop
    shutdown_tx.send(true).unwrap();

    // Wait for all workers to finish
    for handle in handles {
        handle.await.unwrap();
    }

    println!("All workers shut down. Application exiting.");
}
```

## Select

### tokio::select! Macro

Waits on multiple async branches simultaneously and executes the first one that completes. Remaining branches are cancelled:

```rust
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let (tx1, mut rx1) = mpsc::channel::<&str>(1);
    let (tx2, mut rx2) = mpsc::channel::<&str>(1);

    tokio::spawn(async move {
        sleep(Duration::from_millis(200)).await;
        tx1.send("from channel 1").await.unwrap();
    });

    tokio::spawn(async move {
        sleep(Duration::from_millis(100)).await;
        tx2.send("from channel 2").await.unwrap();
    });

    tokio::select! {
        val = rx1.recv() => {
            println!("Received {:?}", val);
        }
        val = rx2.recv() => {
            println!("Received {:?}", val);
        }
    }
}
```

### select! in a Loop

A common pattern for event-driven applications that respond to multiple sources:

```rust
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel::<String>(32);
    let mut tick = interval(Duration::from_secs(2));

    let tx_clone = tx.clone();
    tokio::spawn(async move {
        for i in 0..5 {
            tokio::time::sleep(Duration::from_millis(700)).await;
            tx_clone.send(format!("event-{}", i)).await.unwrap();
        }
        // Sender drops here, rx.recv() will eventually return None
    });
    drop(tx);

    loop {
        tokio::select! {
            Some(msg) = rx.recv() => {
                println!("Event received: {}", msg);
            }
            _ = tick.tick() => {
                println!("Periodic tick - performing maintenance");
            }
            else => {
                println!("All channels closed, exiting loop");
                break;
            }
        }
    }
}
```

### biased select!

Forces deterministic polling order. Branches are checked top-to-bottom instead of randomly. Useful when you want to prioritize certain operations:

```rust
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let (priority_tx, mut priority_rx) = mpsc::channel::<String>(10);
    let (normal_tx, mut normal_rx) = mpsc::channel::<String>(10);

    // Simulate messages arriving on both channels
    let p_tx = priority_tx.clone();
    let n_tx = normal_tx.clone();
    tokio::spawn(async move {
        for i in 0..3 {
            p_tx.send(format!("PRIORITY-{}", i)).await.unwrap();
            n_tx.send(format!("normal-{}", i)).await.unwrap();
            sleep(Duration::from_millis(100)).await;
        }
    });
    drop(priority_tx);
    drop(normal_tx);

    sleep(Duration::from_millis(500)).await;

    loop {
        tokio::select! {
            biased;

            // Priority channel is always checked first
            Some(msg) = priority_rx.recv() => {
                println!("[HIGH] {}", msg);
            }
            Some(msg) = normal_rx.recv() => {
                println!("[LOW]  {}", msg);
            }
            else => break,
        }
    }
}
```

### select! with Preconditions

Disable branches dynamically using `if` guards:

```rust
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel::<i32>(10);
    let mut total = 0i32;
    let limit_reached = false;

    tokio::spawn(async move {
        for i in 1..=10 {
            tx.send(i).await.unwrap();
            sleep(Duration::from_millis(50)).await;
        }
    });

    loop {
        tokio::select! {
            Some(val) = rx.recv(), if !limit_reached => {
                total += val;
                println!("Received {}, total = {}", val, total);
                if total > 20 {
                    println!("Limit reached, stopping receiver");
                    break;
                }
            }
            _ = sleep(Duration::from_secs(5)) => {
                println!("Timeout waiting for data");
                break;
            }
        }
    }
}
```

## Timeouts & Intervals

### timeout

Wrap any future with a deadline. Returns `Err(Elapsed)` if the future does not complete in time:

```rust
use tokio::time::{timeout, Duration};

async fn long_running_operation() -> String {
    tokio::time::sleep(Duration::from_secs(10)).await;
    "completed".to_string()
}

#[tokio::main]
async fn main() {
    match timeout(Duration::from_secs(2), long_running_operation()).await {
        Ok(result) => println!("Operation succeeded: {}", result),
        Err(_) => println!("Operation timed out after 2 seconds"),
    }
}
```

### Nested Timeouts

```rust
use tokio::time::{timeout, Duration};

async fn fetch_data(url: &str) -> Result<String, String> {
    // Simulate network request
    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(format!("Data from {}", url))
}

#[tokio::main]
async fn main() {
    // Overall timeout for the entire operation
    let result = timeout(Duration::from_secs(5), async {
        // Individual request timeout
        let data1 = timeout(Duration::from_secs(2), fetch_data("https://api.example.com/a"))
            .await
            .map_err(|_| "Request 1 timed out".to_string())?
            .map_err(|e| format!("Request 1 failed: {}", e))?;

        let data2 = timeout(Duration::from_secs(2), fetch_data("https://api.example.com/b"))
            .await
            .map_err(|_| "Request 2 timed out".to_string())?
            .map_err(|e| format!("Request 2 failed: {}", e))?;

        Ok::<_, String>(format!("{} | {}", data1, data2))
    })
    .await;

    match result {
        Ok(Ok(data)) => println!("Success: {}", data),
        Ok(Err(e)) => println!("Operation error: {}", e),
        Err(_) => println!("Overall timeout exceeded"),
    }
}
```

### sleep

Pause execution for a specified duration. Does not block the thread; other tasks continue to run:

```rust
use tokio::time::{sleep, Duration, Instant};

#[tokio::main]
async fn main() {
    let start = Instant::now();

    // These sleeps run concurrently when spawned as tasks
    let h1 = tokio::spawn(async {
        sleep(Duration::from_millis(300)).await;
        "A"
    });

    let h2 = tokio::spawn(async {
        sleep(Duration::from_millis(200)).await;
        "B"
    });

    let (a, b) = tokio::join!(h1, h2);
    println!("{} and {} completed in {:?}", a.unwrap(), b.unwrap(), start.elapsed());
    // Prints approximately 300ms, not 500ms, because they ran concurrently
}
```

### sleep_until

Sleep until a specific `Instant` rather than for a duration:

```rust
use tokio::time::{sleep_until, Instant, Duration};

#[tokio::main]
async fn main() {
    let deadline = Instant::now() + Duration::from_secs(2);

    println!("Waiting until deadline...");
    sleep_until(deadline).await;
    println!("Deadline reached");
}
```

### interval

Create a repeating timer that ticks at a fixed rate. Useful for periodic tasks like health checks, metrics collection, and cache cleanup:

```rust
use tokio::time::{interval, Duration, Instant};

#[tokio::main]
async fn main() {
    let mut tick = interval(Duration::from_secs(1));
    let start = Instant::now();

    for _ in 0..5 {
        tick.tick().await;
        println!("Tick at {:?}", start.elapsed());
    }
}
```

### interval with MissedTickBehavior

Control how the interval handles ticks that are delayed beyond the period:

```rust
use tokio::time::{interval, Duration, MissedTickBehavior};

#[tokio::main]
async fn main() {
    let mut tick = interval(Duration::from_millis(100));

    // Skip missed ticks instead of bursting to catch up
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tick.tick().await;
        // Simulate variable-duration work
        let work_duration = rand_duration();
        tokio::time::sleep(work_duration).await;
        println!("Completed work cycle");
    }
}

fn rand_duration() -> Duration {
    Duration::from_millis(50) // Simplified for example
}
```

### Periodic Background Task Pattern

```rust
use tokio::time::{interval, Duration};
use tokio::sync::watch;

async fn run_periodic_cleanup(mut shutdown: watch::Receiver<bool>) {
    let mut cleanup_interval = interval(Duration::from_secs(60));

    loop {
        tokio::select! {
            _ = cleanup_interval.tick() => {
                println!("Running periodic cleanup...");
                // Perform cleanup operations
                cleanup_expired_sessions().await;
                flush_metrics().await;
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    println!("Cleanup task shutting down");
                    return;
                }
            }
        }
    }
}

async fn cleanup_expired_sessions() {
    // Remove expired sessions from the store
}

async fn flush_metrics() {
    // Send accumulated metrics to the monitoring system
}
```

## File Organization

Recommended module structure for a Tokio-based application:

```
src/
├── main.rs                 # Runtime bootstrap, signal handling, top-level orchestration
├── config.rs               # Application configuration (deserialization, validation)
├── server/
│   ├── mod.rs              # Server initialization, TcpListener setup
│   ├── handler.rs          # Per-connection handler logic
│   └── protocol.rs         # Wire protocol parsing (framing, codecs)
├── services/
│   ├── mod.rs              # Service trait definitions
│   ├── auth.rs             # Authentication service
│   └── data.rs             # Data access service
├── channels/
│   ├── mod.rs              # Channel type definitions, Command enums
│   └── dispatcher.rs       # Message routing and dispatch logic
├── tasks/
│   ├── mod.rs              # Task spawning utilities
│   ├── background.rs       # Long-running background tasks (cleanup, sync)
│   └── scheduler.rs        # Periodic task scheduling with intervals
├── sync/
│   ├── mod.rs              # Shared state types
│   └── state.rs            # Application state behind Arc<RwLock<T>> or watch channels
├── shutdown.rs             # Graceful shutdown coordination (broadcast/watch)
└── error.rs                # Application error types and conversions
```

Key structural principles:

- **main.rs** builds the runtime, wires up channels and shared state, spawns top-level tasks, and awaits the shutdown signal.
- **server/** encapsulates all TCP/networking concerns. Each accepted connection is spawned as an independent task.
- **channels/** defines the `Command` enums and message types used for inter-task communication.
- **tasks/** houses spawned background work, keeping it separate from request-handling code.
- **sync/** centralizes all shared state behind appropriate primitives (`Arc<Mutex<T>>`, `Arc<RwLock<T>>`, `watch::Sender`).
- **shutdown.rs** provides a reusable shutdown signal that can be cloned and passed to every long-running task.

## Best Practices

- **Use `#[tokio::main]` for applications and `#[tokio::test]` for tests.** These macros handle runtime construction and teardown. Use the `Builder` API only when you need custom thread counts or names.

- **Never block the async runtime.** Use `tokio::task::spawn_blocking` for CPU-intensive work, file system calls via `std::fs`, or any synchronous library that does not have an async counterpart. Blocking the runtime thread pool starves other tasks.

- **Prefer `std::sync::Mutex` over `tokio::sync::Mutex` when the lock is not held across `.await` points.** The standard library mutex is faster because it does not require async coordination. Only use `tokio::sync::Mutex` when you genuinely need to hold the lock while awaiting.

- **Drop channel senders explicitly when no longer needed.** Receivers rely on all senders being dropped to know that no more messages will arrive. Failing to drop senders causes receivers to hang indefinitely on `recv()`.

- **Use bounded channels and set capacity deliberately.** Unbounded channels can cause unbounded memory growth under load. Choose a capacity that provides sufficient buffering while applying backpressure to producers when the system is saturated.

- **Make spawned tasks self-contained.** Every value moved into a `tokio::spawn` closure must be `Send + 'static`. Design your types around ownership transfer (`Arc`, `clone`) rather than shared references.

- **Implement graceful shutdown using `broadcast` or `watch` channels combined with `tokio::select!`.** Every long-running task should have a shutdown branch that allows it to finish current work and clean up resources before the process exits.

- **Use `tokio::select!` carefully with cancel safety in mind.** When a branch is cancelled, any partially completed work in that future is lost. For operations that must run to completion (such as writing a full message to a socket), use `tokio::pin!` and resume the future across loop iterations rather than restarting it.

- **Prefer `JoinSet` over manually collecting `Vec<JoinHandle>`.** `JoinSet` provides built-in support for awaiting tasks as they complete, aborting remaining tasks, and avoiding boilerplate.

- **Structure timeouts at multiple levels.** Apply an overall request timeout at the outer boundary and per-operation timeouts at the inner boundary. Use `tokio::time::timeout` for individual operations and `tokio::select!` with `tokio::time::sleep` for loop-based deadline enforcement.

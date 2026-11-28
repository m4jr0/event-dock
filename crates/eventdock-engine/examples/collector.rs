use eventdock_engine::{Backend, BackendConfig};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

fn main() -> io::Result<()> {
    let (backend, stop) = initialize()?;

    println!("Listening on {}", backend.address());
    println!("Press Ctrl+C to stop");

    run(&backend, &stop);

    println!("Final: {:?}", backend.shutdown()?);
    Ok(())
}

fn initialize() -> io::Result<(Backend, Arc<AtomicBool>)> {
    let stop = Arc::new(AtomicBool::new(false));
    let handler_stop = Arc::clone(&stop);

    ctrlc::set_handler(move || handler_stop.store(true, Ordering::Relaxed))
        .map_err(io::Error::other)?;

    let backend = Backend::start(BackendConfig::default())?;
    Ok((backend, stop))
}

fn run(
    backend: &Backend,
    stop: &AtomicBool,
) {
    while backend.is_running() && !stop.load(Ordering::Relaxed) {
        println!("{:?}", backend.snapshot());
        thread::sleep(Duration::from_secs(1));
    }
}

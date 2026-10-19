use super::{ModelError, ResourceLimits};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type Response = Result<Vec<u8>, ()>;

pub(super) struct Worker {
    child: Child,
    requests: Option<SyncSender<Vec<u8>>>,
    responses: Receiver<Response>,
    io_thread: Option<JoinHandle<()>>,
    cleanup_timeout: Duration,
    pub(super) cleanup_failed: Arc<AtomicBool>,
}

impl Worker {
    pub fn spawn(
        executable: &std::path::Path,
        bundle: &std::path::Path,
        limits: &ResourceLimits,
    ) -> Result<Self, ModelError> {
        let mut child = Command::new(executable)
            .arg(bundle)
            .env_remove("ORT_DYLIB_PATH")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| ModelError::new("model_load_failed", "runtime_unavailable", "/bundle"))?;
        let Some(mut input) = child.stdin.take() else {
            return Err(Self::abort_spawn(&mut child));
        };
        let Some(output) = child.stdout.take() else {
            return Err(Self::abort_spawn(&mut child));
        };
        let (requests, receive) = mpsc::sync_channel::<Vec<u8>>(1);
        let (send, responses) = mpsc::sync_channel(1);
        let max_response = limits.max_response_bytes;
        let io_thread = thread::Builder::new()
            .name("datafog-model-io".into())
            .spawn(move || {
                let mut output = BufReader::new(output);
                while let Ok(payload) = receive.recv() {
                    let response = (|| {
                        input.write_all(&payload).map_err(|_| ())?;
                        input.write_all(b"\n").map_err(|_| ())?;
                        input.flush().map_err(|_| ())?;
                        let mut bytes = Vec::new();
                        output
                            .by_ref()
                            .take(max_response as u64 + 1)
                            .read_until(b'\n', &mut bytes)
                            .map_err(|_| ())?;
                        if bytes.is_empty()
                            || bytes.len() > max_response
                            || bytes.last() != Some(&b'\n')
                        {
                            return Err(());
                        }
                        Ok(bytes)
                    })();
                    let failed = response.is_err();
                    if send.send(response).is_err() || failed {
                        break;
                    }
                }
            });
        let io_thread = match io_thread {
            Ok(handle) => handle,
            Err(_) => return Err(Self::abort_spawn(&mut child)),
        };
        Ok(Self {
            child,
            requests: Some(requests),
            responses,
            io_thread: Some(io_thread),
            cleanup_timeout: Duration::from_millis(limits.cleanup_timeout_ms),
            cleanup_failed: Arc::new(AtomicBool::new(false)),
        })
    }

    fn abort_spawn(child: &mut Child) -> ModelError {
        if child.kill().is_err() || child.wait().is_err() {
            ModelError::new("model_cleanup_failed", "process_cleanup_failed", "/backend")
        } else {
            ModelError::new("model_load_failed", "runtime_unavailable", "/backend")
        }
    }

    pub fn exchange(
        &mut self,
        text: &str,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Vec<u8>, ModelError> {
        let payload = serde_json::to_vec(&serde_json::json!({ "text": text }))
            .map_err(|_| ModelError::invalid_output())?;
        self.requests
            .as_ref()
            .ok_or_else(ModelError::closed)?
            .try_send(payload)
            .map_err(|_| ModelError::invalid_output())?;
        loop {
            if cancelled() {
                return Err(ModelError::new("model_cancelled", "cancelled", "/data"));
            }
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(ModelError::timeout)?;
            // Polling only controls cancellation responsiveness, not a request limit.
            match self
                .responses
                .recv_timeout(remaining.min(Duration::from_millis(10)))
            {
                Ok(Ok(response)) => return Ok(response),
                Ok(Err(())) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(ModelError::invalid_output());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    pub fn stop(&mut self) -> Result<(), ModelError> {
        let deadline = Instant::now()
            .checked_add(self.cleanup_timeout)
            .ok_or_else(ModelError::cleanup)?;
        self.requests.take();
        if self
            .child
            .try_wait()
            .map_err(|_| ModelError::cleanup())?
            .is_none()
        {
            if self.child.kill().is_err()
                && self
                    .child
                    .try_wait()
                    .map_err(|_| ModelError::cleanup())?
                    .is_none()
            {
                return Err(ModelError::cleanup());
            }
            while self
                .child
                .try_wait()
                .map_err(|_| ModelError::cleanup())?
                .is_none()
            {
                if Instant::now() >= deadline {
                    return Err(ModelError::cleanup());
                }
                thread::sleep(Duration::from_millis(1));
            }
        }
        if let Some(handle) = self.io_thread.as_ref() {
            while !handle.is_finished() {
                if Instant::now() >= deadline {
                    return Err(ModelError::cleanup());
                }
                thread::sleep(Duration::from_millis(1));
            }
        }
        if let Some(handle) = self.io_thread.take() {
            handle.join().map_err(|_| ModelError::cleanup())?;
        }
        Ok(())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Destructors cannot return errors. Record cleanup failure for session inspection.
        if self.stop().is_err() {
            self.cleanup_failed.store(true, Ordering::Release);
        }
    }
}

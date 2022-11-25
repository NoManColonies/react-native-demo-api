use std::sync::Arc;
use tokio::sync::{mpsc, Notify};
use tokio_stream::Stream;
use tracing::debug;

#[derive(Debug)]
/// this struct represent `tokio_stream::Stream` that will send `tokio::sync::Notify::notified()`
/// once when the struct is dropped. This struct will be dropped automatically when client
/// explicitly cancel the stream or client connection get dropped
pub struct ClientCancellableStream<T> {
    notifier: Arc<Notify>,
    inner: mpsc::Receiver<T>,
}

impl<T> ClientCancellableStream<T> {
    pub fn new() -> (mpsc::Sender<T>, Self, Arc<Notify>) {
        let (stream_data_pusher, stream_data_receiver) = mpsc::channel::<T>(4);
        let client_cancellation_signal_notifier = Arc::new(Notify::new());

        (
            stream_data_pusher,
            ClientCancellableStream {
                notifier: Arc::clone(&client_cancellation_signal_notifier),
                inner: stream_data_receiver,
            },
            client_cancellation_signal_notifier,
        )
    }
}

impl<T> Stream for ClientCancellableStream<T> {
    type Item = T;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.poll_recv(cx)
    }
}

impl<T> Drop for ClientCancellableStream<T> {
    fn drop(&mut self) {
        self.notifier.notify_one();
        debug!("client dropped stream");
    }
}

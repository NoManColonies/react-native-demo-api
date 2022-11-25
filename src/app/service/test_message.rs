use self::test_message::EventConfigRequest;
use crate::app::{config::task::spawn_with_name, util::stream::ClientCancellableStream};
use futures::{Stream, StreamExt};
use redis::aio::ConnectionManager;
use sentry::{Hub, SentryFutureExt};
use std::sync::Arc;
use std::time::Duration;
use test_message::{test_message_service_server::TestMessageService, ResponseMessage, TestMessage};
use tokio::{sync::Notify, time::sleep};
use tonic::{Request, Response, Status, Streaming};
use tracing::{error, info};
use tracing_futures::Instrument;

pub mod test_message {
    tonic::include_proto!("test_message");
}

pub struct TestMessageGreeter {
    pub(crate) shutdown_signal_notifier: Arc<Notify>,
    pub(crate) redis_pool: ConnectionManager,
}

#[tonic::async_trait]
impl TestMessageService for TestMessageGreeter {
    type ChatMessageStream = ClientCancellableStream<Result<ResponseMessage, Status>>;
    type EventMessageStream = ClientCancellableStream<Result<ResponseMessage, Status>>;

    async fn send_message(
        &self,
        request: Request<TestMessage>,
    ) -> Result<Response<ResponseMessage>, Status> {
        Ok(Response::new(ResponseMessage {
            content: request.into_inner().content,
        }))
    }

    async fn stream_message(
        &self,
        request: Request<Streaming<TestMessage>>,
    ) -> Result<Response<ResponseMessage>, Status> {
        let mut buffer: Vec<String> = vec![];
        let mut stream = request.into_inner();

        while let Some(message) = stream.next().await {
            if let Ok(message) = message {
                info!(message.content);
                buffer.push(message.content);
            }
        }

        Ok(Response::new(ResponseMessage {
            content: buffer.join(","),
        }))
    }

    async fn event_message(
        &self,
        request: Request<EventConfigRequest>,
    ) -> Result<Response<Self::EventMessageStream>, Status> {
        let (responder, response_stream, ..) = ClientCancellableStream::new();
        let config = request.into_inner();
        let hub = Hub::current();

        spawn_with_name(
            async move {
                for round in 0..config.count {
                    sleep(Duration::from_millis(config.delay as u64)).await;
                    if let Err(error) = responder
                        .send(Ok(ResponseMessage {
                            content: format!("message: {}", round + 1),
                        }))
                        .await
                    {
                        error!("response failed: {}", error);
                    }
                }
            }
            .in_current_span()
            .bind_hub(hub),
            "server_stream",
        );

        Ok(Response::new(response_stream))
    }

    async fn chat_message(
        &self,
        request: Request<Streaming<TestMessage>>,
    ) -> Result<Response<Self::ChatMessageStream>, Status> {
        let mut stream = request.into_inner();
        let (responder, response_stream, ..) = ClientCancellableStream::new();
        let hub = Hub::current();

        spawn_with_name(
            async move {
                while let Some(message) = stream.next().await {
                    if let Ok(message) = message {
                        if let Err(error) = responder
                            .send(Ok(ResponseMessage {
                                content: message.content,
                            }))
                            .await
                        {
                            error!("response failed: {}", error);
                        }
                    }
                }
            }
            .bind_hub(hub)
            .in_current_span(),
            "client_stream",
        );

        Ok(Response::new(response_stream))
    }
}

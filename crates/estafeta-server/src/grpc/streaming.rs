use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::info;

use estafeta_proto::estafeta::v1::{
    streaming_service_server::StreamingService as StreamingServiceTrait, NewNotification,
    NotificationEvent, StateChange, SubscribeRequest,
};

use crate::auth::AuthClaims;
use crate::lifecycle::state_machine::NotificationState;
use crate::nats::RealtimeEvent;

use super::proto_convert;

pub struct StreamingServiceImpl {
    nats_client: async_nats::Client,
}

impl StreamingServiceImpl {
    pub fn new(nats_client: async_nats::Client) -> Self {
        Self { nats_client }
    }
}

#[tonic::async_trait]
impl StreamingServiceTrait for StreamingServiceImpl {
    type SubscribeStream = ReceiverStream<Result<NotificationEvent, Status>>;

    async fn subscribe(
        &self,
        request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let user_id = subject;
        let (tx, rx) = mpsc::channel(128);

        // Subscribe to real-time NATS Core subjects for this user
        let new_notif_subject = format!("rt.user.{user_id}");
        let state_change_subject = format!("rt.user.{user_id}.state");

        let mut new_sub = self
            .nats_client
            .subscribe(new_notif_subject.clone())
            .await
            .map_err(|e| Status::internal(format!("nats subscribe failed: {e}")))?;

        let mut state_sub = if req.include_state_changes {
            Some(
                self.nats_client
                    .subscribe(state_change_subject.clone())
                    .await
                    .map_err(|e| Status::internal(format!("nats subscribe failed: {e}")))?,
            )
        } else {
            None
        };

        let service_filter: Vec<String> = req.service_slugs;
        let _include_state_changes = req.include_state_changes;

        info!(user_id = %user_id, "client subscribed to notification stream");

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(msg) = new_sub.next() => {
                        if let Ok(event) = serde_json::from_slice::<RealtimeEvent>(&msg.payload) {
                            // Apply service filter
                            if !service_filter.is_empty()
                                && !service_filter.contains(&event.service_slug)
                            {
                                continue;
                            }

                            let proto_event = NotificationEvent {
                                event: Some(
                                    estafeta_proto::estafeta::v1::notification_event::Event::NewNotification(
                                        NewNotification {
                                            id: event.notification_id.to_string(),
                                            service_slug: event.service_slug,
                                            notification_type: event.notification_type,
                                            level: event.level.unwrap_or_default(),
                                            payload: event.payload.as_ref().and_then(|p| proto_convert::value_to_proto_struct(p)),
                                            group_key: event.group_key.unwrap_or_default(),
                                            metadata: event.metadata.unwrap_or_default(),
                                            created_at: None,
                                            expires_at: None,
                                            action_url: event.action_url.unwrap_or_default(),
                                            icon: event.icon.unwrap_or_default(),
                                        },
                                    ),
                                ),
                            };

                            if tx.send(Ok(proto_event)).await.is_err() {
                                break; // client disconnected
                            }
                        }
                    }
                    Some(msg) = async {
                        if let Some(ref mut sub) = state_sub {
                            sub.next().await
                        } else {
                            std::future::pending::<Option<async_nats::Message>>().await
                        }
                    } => {
                        if let Ok(event) = serde_json::from_slice::<RealtimeEvent>(&msg.payload) {
                            let proto_event = NotificationEvent {
                                event: Some(
                                    estafeta_proto::estafeta::v1::notification_event::Event::StateChange(
                                        StateChange {
                                            notification_id: event.notification_id.to_string(),
                                            old_state: NotificationState::from_db_str(
                                                event.old_state.as_deref().unwrap_or(""),
                                            )
                                            .map(|s| s.to_proto())
                                            .unwrap_or(0),
                                            new_state: NotificationState::from_db_str(
                                                event.new_state.as_deref().unwrap_or(""),
                                            )
                                            .map(|s| s.to_proto())
                                            .unwrap_or(0),
                                            changed_at: None,
                                        },
                                    ),
                                ),
                            };

                            if tx.send(Ok(proto_event)).await.is_err() {
                                break;
                            }
                        }
                    }
                    else => break,
                }
            }
            info!(user_id = %user_id, "client disconnected from notification stream");
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

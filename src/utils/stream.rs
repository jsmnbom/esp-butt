use async_stream::stream;
use futures::Stream;
use tokio::sync::broadcast;

pub fn convert_broadcast_receiver_to_stream<T>(
  mut receiver: broadcast::Receiver<T>,
) -> impl Stream<Item = T>
where
  T: Unpin + Clone,
{
  stream! {
    loop {
      match receiver.recv().await {
        Ok(val) => yield val,
        Err(broadcast::error::RecvError::Lagged(_)) => {
          // If we've lagged behind, we can just skip the missed messages and continue with the latest one
          continue;
        }
        Err(broadcast::error::RecvError::Closed) => {
          // If the channel is closed, we can end the stream
          break;
        }
      }
    }
  }
}

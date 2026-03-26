use std::sync::{
  Arc,
  atomic::{AtomicBool, Ordering},
};

use buttplug_core::{
  connector::{ButtplugConnector, ButtplugConnectorError, ButtplugConnectorResultFuture},
  message::{ButtplugClientMessageV4, ButtplugServerMessageV4},
};
use buttplug_server::{ButtplugServer, message::ButtplugServerMessageVariant};
use futures::{
  FutureExt, StreamExt, future::{self, BoxFuture}
};
use log::info;
use tokio::sync::mpsc;

use crate::utils::task::{Core, spawn};

pub struct SimpleInProcessClientConnector {
  server: Arc<ButtplugServer>,
  server_outbound_sender: Option<mpsc::Sender<ButtplugServerMessageV4>>,
  connected: Arc<AtomicBool>,
}

impl SimpleInProcessClientConnector {
  pub fn new(server: Arc<ButtplugServer>) -> Self {
    Self {
      server,
      server_outbound_sender: None,
      connected: Arc::new(AtomicBool::new(false)),
    }
  }
}

impl ButtplugConnector<ButtplugClientMessageV4, ButtplugServerMessageV4>
  for SimpleInProcessClientConnector
{
  fn connect(
    &mut self,
    message_sender: mpsc::Sender<ButtplugServerMessageV4>,
  ) -> BoxFuture<'static, Result<(), ButtplugConnectorError>> {
    if !self.connected.load(Ordering::Relaxed) {
      let connected = self.connected.clone();
      let send = message_sender.clone();
      self.server_outbound_sender = Some(message_sender);
      let mut server_recv = Box::pin(self.server.server_version_event_stream());
      async move {
        spawn(
          async move {
            info!("Starting In Process Client Connector Event Sender Loop");
            while let Some(event) = server_recv.next().await {
              if send.send(event).await.is_err() {
                break;
              }
            }
            info!("Stopping In Process Client Connector Event Sender Loop, due to channel receiver being dropped.");
          },
          c"InProcessClientConnectorEventSenderLoop",
          8192,
          Core::App,
          5
        );
        connected.store(true, Ordering::Relaxed);
        Ok(())
      }.boxed()
    } else {
      ButtplugConnectorError::ConnectorAlreadyConnected.into()
    }
  }

  fn disconnect(&self) -> ButtplugConnectorResultFuture {
    if self.connected.load(Ordering::Relaxed) {
      self.connected.store(false, Ordering::Relaxed);
      future::ready(Ok(())).boxed()
    } else {
      ButtplugConnectorError::ConnectorNotConnected.into()
    }
  }

  fn send(&self, msg: ButtplugClientMessageV4) -> ButtplugConnectorResultFuture {
    if !self.connected.load(Ordering::Relaxed) {
      return ButtplugConnectorError::ConnectorNotConnected.into();
    }
    let input = msg.into();
    let output_fut = self.server.parse_message(input);
    let sender = self.server_outbound_sender.clone();
    async move {
      let output = match output_fut.await {
        Ok(m) => {
          if let ButtplugServerMessageVariant::V4(msg) = m {
            msg
          } else {
            unreachable!("In-process connector messages should never have differing versions.")
          }
        }
        Err(e) => {
          if let ButtplugServerMessageVariant::V4(msg) = e {
            msg
          } else {
            unreachable!("In-process connector messages should never have differing versions.")
          }
        }
      };
      if let Some(sender) = sender {
        sender
          .send(output)
          .await
          .map_err(|_| ButtplugConnectorError::ConnectorNotConnected)
      } else {
        Err(ButtplugConnectorError::ConnectorNotConnected)
      }
    }
    .boxed()
  }
}

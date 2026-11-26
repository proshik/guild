//! Cap each blocking input wait while preserving ureq's existing TLS/HTTP stack.
//! ureq's recv_body timeout is a TOTAL body budget, not an idle/read timeout.
//! This uses its public unversioned transport adapter; the workspace pins ureq.
use std::time::Duration;
use ureq::unversioned::{
    resolver::DefaultResolver,
    transport::{Buffers, ConnectionDetails, Connector, NextTimeout, Transport},
};
#[derive(Debug)]
struct IdleConnector;
#[derive(Debug)]
struct IdleTransport(Box<dyn Transport>);
impl Connector<Box<dyn Transport>> for IdleConnector {
    type Out = IdleTransport;
    fn connect(
        &self,
        _details: &ConnectionDetails,
        chained: Option<Box<dyn Transport>>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(chained.map(IdleTransport))
    }
}
impl Transport for IdleTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.0.buffers()
    }
    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.0.transmit_output(amount, timeout)
    }
    fn await_input(&mut self, mut timeout: NextTimeout) -> Result<bool, ureq::Error> {
        timeout.after = timeout.after.min(Duration::from_secs(1).into());
        self.0.await_input(timeout)
    }
    fn is_open(&mut self) -> bool {
        self.0.is_open()
    }
    fn is_tls(&self) -> bool {
        self.0.is_tls()
    }
}
pub(super) fn agent(
    deadline: Duration,
    network: &guild_session::network::Network,
) -> Result<ureq::Agent, super::Error> {
    let config = ureq::Agent::config_builder()
        .proxy(crate::network::http_policy(network).map_err(|_| super::Error::Invalid)?)
        .timeout_global(Some(deadline))
        .max_redirects(0)
        .http_status_as_error(false)
        .build();
    Ok(ureq::Agent::with_parts(
        config,
        crate::network::http_connector(network)
            .map_err(|_| super::Error::Invalid)?
            .chain(IdleConnector),
        DefaultResolver::default(),
    ))
}

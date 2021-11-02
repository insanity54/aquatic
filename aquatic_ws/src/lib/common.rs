use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use aquatic_common::access_list::{create_access_list_cache, AccessListArcSwap, AccessListCache};
use hashbrown::HashMap;
use indexmap::IndexMap;

pub use aquatic_common::ValidUntil;

use aquatic_ws_protocol::*;

use crate::config::Config;

pub type TlsConfig = futures_rustls::rustls::ServerConfig;

#[derive(Copy, Clone, Debug)]
pub struct PendingScrapeId(pub usize);

#[derive(Copy, Clone, Debug)]
pub struct ConsumerId(pub usize);

#[derive(Clone, Copy, Debug)]
pub struct ConnectionId(pub usize);

#[derive(Clone, Copy, Debug)]
pub struct ConnectionMeta {
    /// Index of socket worker responsible for this connection. Required for
    /// sending back response through correct channel to correct worker.
    pub out_message_consumer_id: ConsumerId,
    pub connection_id: ConnectionId,
    /// Peer address as received from socket, meaning it wasn't converted to
    /// an IPv4 address if it was a IPv4-mapped IPv6 address
    pub naive_peer_addr: SocketAddr,
    pub converted_peer_ip: IpAddr,
    pub pending_scrape_id: Option<PendingScrapeId>,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum PeerStatus {
    Seeding,
    Leeching,
    Stopped,
}

impl PeerStatus {
    /// Determine peer status from announce event and number of bytes left.
    ///
    /// Likely, the last branch will be taken most of the time.
    #[inline]
    pub fn from_event_and_bytes_left(event: AnnounceEvent, opt_bytes_left: Option<usize>) -> Self {
        if let AnnounceEvent::Stopped = event {
            Self::Stopped
        } else if let Some(0) = opt_bytes_left {
            Self::Seeding
        } else {
            Self::Leeching
        }
    }
}

#[derive(Clone, Copy)]
pub struct Peer {
    pub connection_meta: ConnectionMeta,
    pub status: PeerStatus,
    pub valid_until: ValidUntil,
}

pub type PeerMap = IndexMap<PeerId, Peer>;

pub struct TorrentData {
    pub peers: PeerMap,
    pub num_seeders: usize,
    pub num_leechers: usize,
}

impl Default for TorrentData {
    #[inline]
    fn default() -> Self {
        Self {
            peers: IndexMap::new(),
            num_seeders: 0,
            num_leechers: 0,
        }
    }
}

pub type TorrentMap = HashMap<InfoHash, TorrentData>;

#[derive(Default)]
pub struct TorrentMaps {
    pub ipv4: TorrentMap,
    pub ipv6: TorrentMap,
}

impl TorrentMaps {
    pub fn clean(&mut self, config: &Config, access_list: &Arc<AccessListArcSwap>) {
        let mut access_list_cache = create_access_list_cache(access_list);

        Self::clean_torrent_map(config, &mut access_list_cache, &mut self.ipv4);
        Self::clean_torrent_map(config, &mut access_list_cache, &mut self.ipv6);
    }

    fn clean_torrent_map(
        config: &Config,
        access_list_cache: &mut AccessListCache,
        torrent_map: &mut TorrentMap,
    ) {
        let now = Instant::now();

        torrent_map.retain(|info_hash, torrent_data| {
            if !access_list_cache
                .load()
                .allows(config.access_list.mode, &info_hash.0)
            {
                return false;
            }

            let num_seeders = &mut torrent_data.num_seeders;
            let num_leechers = &mut torrent_data.num_leechers;

            torrent_data.peers.retain(|_, peer| {
                let keep = peer.valid_until.0 >= now;

                if !keep {
                    match peer.status {
                        PeerStatus::Seeding => {
                            *num_seeders -= 1;
                        }
                        PeerStatus::Leeching => {
                            *num_leechers -= 1;
                        }
                        _ => (),
                    };
                }

                keep
            });

            !torrent_data.peers.is_empty()
        });

        torrent_map.shrink_to_fit();
    }
}

#[derive(Default, Clone)]
pub struct State {
    pub access_list: Arc<AccessListArcSwap>,
}

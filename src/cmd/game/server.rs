use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    str::FromStr,
    time::Duration,
};

use clap::{ValueEnum, crate_version};
use color_print::cformat;
use comfy_table::{
    Attribute, Cell, Color, Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL,
};
use futures_util::future::join_all;
use rand::random;
use surge_ping::{Client as PingClient, Config as PingConfig, PingIdentifier, PingSequence};

use crate::{
    cmd::{Run, ServerInfo},
    errors::{Error, TResult},
    game::Game,
};

impl Run for ServerInfo {
    async fn run(&self) -> TResult<()> {
        let reqwest_client = reqwest::Client::builder()
            .user_agent(format!("Yet Another TruckersMP Cli/{:?}", crate_version!()))
            .build()?;
        let servers: Vec<Server> = get_servers(&reqwest_client).await?.into();

        let servers = if let Some(game) = self.game {
            servers
                .into_iter()
                .filter(|s| s.game == game)
                .collect::<Vec<_>>()
        } else {
            servers
        };

        let servers: Vec<Server> = if !self.server_type.contains(&ServerInfoType::All) {
            servers
                .into_iter()
                .filter(|s| s.server_type.iter().any(|st| self.server_type.contains(st)))
                .collect()
        } else {
            servers
        };

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS);

        let mut default_header = vec![
            "Server Name",
            "Game",
            "Type",
            "Players", // should include queue ones
        ];

        if self.speed_limit {
            default_header.push("Speed Limit");
        }
        if self.ping {
            default_header.push("Ping");
        }
        if self.additional {
            default_header.push("Additional");
        }

        table.set_header(default_header);

        let mut ping_tasks = Vec::new();

        let pinger_v4 = surge_ping::Client::new(&PingConfig::default())?;
        let pinger_v6 =
            surge_ping::Client::new(&PingConfig::builder().kind(surge_ping::ICMP::V6).build())?;

        let unique_ips: HashSet<IpAddr> = servers.iter().map(|s| s.ip).collect();

        for unique_ip in &unique_ips {
            if unique_ip.is_ipv4() {
                ping_tasks.push(tokio::spawn(ping(pinger_v4.clone(), *unique_ip)))
            } else if unique_ip.is_ipv6() {
                ping_tasks.push(tokio::spawn(ping(pinger_v6.clone(), *unique_ip)))
            } else {
                unreachable!("Invalid IP address format: {}", unique_ip);
            }
        }

        let ping_results = join_all(ping_tasks).await;
        let mut ping_map = HashMap::with_capacity(unique_ips.len());

        for (unique_ip, ping_result) in unique_ips.iter().zip(ping_results) {
            let ping_result = ping_result?;
            ping_map.insert(unique_ip, ping_result);
        }

        for server in servers {
            let ping = ping_map.get(&server.ip).copied().unwrap().1;
            let types = server
                .server_type
                .iter()
                .map(|t| format!("{t:?}"))
                .collect::<Vec<_>>()
                .join(", ");

            let mut row = vec![
                Cell::new(server.name)
                    .add_attribute((|| {
                        if server.online {
                            Attribute::NormalIntensity
                        } else {
                            Attribute::Dim
                        }
                    })())
                    .fg((|| {
                        if server.online {
                            Color::Green
                        } else {
                            Color::Red
                        }
                    })()),
                Cell::new(format!("{:?}", server.game)),
                Cell::new(types),
                Cell::new(format!(
                    "{}/{} {}",
                    server.players,
                    server.max_players,
                    if server.player_queue > 0 {
                        format!("(+{})", server.player_queue)
                    } else {
                        "".to_string()
                    }
                )),
            ];

            if self.speed_limit {
                row.push(Cell::new(format!(
                    "{} {}",
                    server.speed_limit.to_str(),
                    if server.speed_limit == Speedlimit::Disabled {
                        ""
                    } else {
                        "km/h"
                    }
                )));
            }
            if self.ping {
                row.push(Cell::new(format!("{} ms", ping)));
            }
            if self.additional {
                row.push(Cell::new(format!(
                    "{}\n{}\n{}\n{}",
                    if server.afk_enabled {
                        cformat!("<green!>AFK</>")
                    } else {
                        cformat!("<dim,italic>AFK</>")
                    },
                    if server.cars_for_players {
                        cformat!("<green!>Cars</>")
                    } else {
                        cformat!("<dim,italic>Cars</>")
                    },
                    if server.police_cars_for_players {
                        cformat!("<green!>Police Cars</>")
                    } else {
                        cformat!("<dim,italic>Police Cars</>")
                    },
                    if server.collisions {
                        cformat!("<green!>Collisions</>")
                    } else {
                        cformat!("<dim,italic>Collisions</>")
                    }
                )));
            }

            table.add_row(row);
        }

        println!("{table}");

        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum ServerInfoType {
    Simulation,
    Arcade,
    Promods,
    Event,
    #[clap(hide = true)]
    Unknown,
    #[default]
    All,
}

#[derive(Debug, PartialEq, Eq)]
enum Speedlimit {
    High,
    Normal,
    Disabled,
}

impl Speedlimit {
    fn to_str(&self) -> &'static str {
        match self {
            Speedlimit::High => "60/110",
            Speedlimit::Normal => "80/150",
            Speedlimit::Disabled => "Disabled",
        }
    }
}

// unnecessary conversion but whatever lol
#[derive(Debug)]
struct Server {
    game: Game,
    server_type: Vec<ServerInfoType>,
    name: String,
    players: u32,
    max_players: u32,
    player_queue: u32,
    speed_limit: Speedlimit,
    collisions: bool,
    cars_for_players: bool,
    police_cars_for_players: bool,
    afk_enabled: bool,
    online: bool,
    ip: IpAddr,
}

impl From<RawServer> for Server {
    fn from(value: RawServer) -> Self {
        let mut server_type = Vec::new();

        if value.promods {
            server_type.push(ServerInfoType::Promods)
        }

        if value.short_name.contains("SIM") {
            server_type.push(ServerInfoType::Simulation)
        }

        if value.short_name.contains("ARC") {
            server_type.push(ServerInfoType::Arcade);
        }

        if value.event {
            server_type.push(ServerInfoType::Event);
        }

        if server_type.is_empty() {
            server_type.push(ServerInfoType::Unknown);
        }

        let speed_limit = if value.speed_limiter == 0 {
            if value.short_name.contains("ARC") {
                Speedlimit::Disabled
            } else {
                Speedlimit::Normal
            }
        } else {
            Speedlimit::High
        };

        Self {
            game: value.game,
            server_type,
            name: value.name,
            players: value.players,
            max_players: value.max_players,
            player_queue: value.queue,
            speed_limit,
            collisions: value.collisions,
            cars_for_players: value.cars_for_players,
            police_cars_for_players: value.police_cars_for_players,
            afk_enabled: value.afk_enabled,
            online: value.online,
            ip: IpAddr::from_str(&value.ip).unwrap_or_else(|_| IpAddr::V4([0, 0, 0, 0].into())),
        }
    }
}

impl From<RawServerInfo> for Vec<Server> {
    fn from(value: RawServerInfo) -> Self {
        value.response.into_iter().map(Server::from).collect()
    }
}

#[derive(Debug, serde::Deserialize)]
struct RawServerInfo {
    #[serde(deserialize_with = "deserialize_bool")]
    error: bool, // making it a a string when the other things are bool is just a sin :(
    response: Vec<RawServer>,
}

fn deserialize_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let s: &str = serde::de::Deserialize::deserialize(deserializer)?;

    match s {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(serde::de::Error::unknown_variant(s, &["true", "false"])),
    }
}

#[derive(Debug, serde::Deserialize)]
struct RawServer {
    // id: u32,
    game: Game,
    ip: String,
    // port: u16,
    name: String,
    #[serde(rename = "shortname")]
    short_name: String,
    // #[serde(rename = "idprefix")]
    // id_prefix: Option<String>,
    online: bool,
    players: u32,
    queue: u32,
    #[serde(rename = "maxplayers")]
    max_players: u32,
    // #[serde(rename = "mapid")]
    // map_id: u32,
    // #[serde(rename = "displayorder")]
    // display_order: u32,
    #[serde(rename = "speedlimiter")]
    speed_limiter: u32,
    collisions: bool,
    #[serde(rename = "carsforplayers")]
    cars_for_players: bool,
    #[serde(rename = "policecarsforplayers")]
    police_cars_for_players: bool,
    #[serde(rename = "afkenabled")]
    afk_enabled: bool,
    event: bool,
    // #[serde(rename = "specialEvent")] // who made this api response...
    // special_event: bool,
    promods: bool,
    // #[serde(rename = "syncdelay")]
    // sync_delay: u32,
}

async fn get_servers(client: &reqwest::Client) -> TResult<RawServerInfo> {
    let result: RawServerInfo = client
        .get("https://api.truckersmp.com/v2/servers")
        .send()
        .await?
        .json()
        .await?;

    if result.error {
        return Err(Error::TruckersMPError);
    }

    // println!("raww: {:#?}", result);

    Ok(result)
}

// single ping pong action
async fn ping(client: PingClient, addr: IpAddr) -> (IpAddr, u32) {
    let payload = [0; 56];
    let mut pinger = client.pinger(addr, PingIdentifier(random())).await;
    pinger.timeout(Duration::from_secs(1));
    match pinger.ping(PingSequence(0), &payload).await {
        Ok((_, dur)) => (addr, dur.as_millis() as u32),
        Err(_) => (addr, 9999),
    }
}

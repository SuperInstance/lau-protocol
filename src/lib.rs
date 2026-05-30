//! # lau-protocol
//!
//! Layered Agent-UI protocol — bridges PLATO room agents to external game
//! engines and UIs. "Lau" = Layered Agent-UI. It ports agents IN (external
//! input → PLATO room) and OUT (PLATO room → game avatar).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// LauMessage — universal message format
// ---------------------------------------------------------------------------

/// The universal message format for the Layered Agent-UI protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LauMessage {
    /// Agent does something.
    Action {
        agent_id: String,
        action: String,
        params: HashMap<String, f64>,
    },
    /// Agent sees something.
    Observe {
        agent_id: String,
        sensor: String,
        value: f64,
        confidence: f64,
    },
    /// Agent says something (emotion 0–1).
    Speak {
        agent_id: String,
        text: String,
        emotion: f64,
    },
    /// Agent moves in 3D space.
    Move {
        agent_id: String,
        x: f64,
        y: f64,
        z: f64,
        facing: f64,
    },
    /// Agent interacts with object / NPC.
    Interact {
        agent_id: String,
        target_id: String,
        kind: String,
    },
    /// Room state broadcast.
    StateUpdate {
        room_id: String,
        vibe: f64,
        population: usize,
        events: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// AvatarAppearance & AvatarMapping
// ---------------------------------------------------------------------------

/// Visual properties of a game avatar derived from PLATO room state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvatarAppearance {
    pub color: (f64, f64, f64),
    pub scale: f64,
    pub glow: f64,
    pub animation: String,
    pub opacity: f64,
}

/// Maps PLATO room state to game avatar properties.
pub struct AvatarMapping;

impl AvatarMapping {
    /// Convert room vibe (0–1) to an `AvatarAppearance`.
    ///
    /// * Low vibe (0) → blue, small, no glow
    /// * Mid vibe (0.5) → green, normal, moderate glow
    /// * High vibe (1) → gold/warm, large, bright glow
    pub fn room_vibe_to_appearance(vibe: f64) -> AvatarAppearance {
        let v = vibe.clamp(0.0, 1.0);
        let color = (
            v,                          // R: rises with vibe
            0.8 - 0.4 * (v - 0.5).abs(), // G: peaks at mid-vibe
            1.0 - v,                    // B: falls with vibe
        );
        AvatarAppearance {
            color,
            scale: 0.5 + v,
            glow: v,
            animation: String::new(), // caller sets via phase
            opacity: 1.0,
        }
    }

    /// Map a PLATO room phase to an animation name.
    pub fn room_phase_to_animation(phase: &str) -> String {
        match phase {
            "Gestating" => "idle".into(),
            "Forming" => "exploring".into(),
            "Stable" => "confident".into(),
            "Dissolving" => "fading".into(),
            other => other.to_lowercase(),
        }
    }

    /// Convert observation confidence (0–1) to visibility (opacity).
    ///
    /// Low confidence → transparent / ghostly; high → fully visible.
    pub fn confidence_to_visibility(confidence: f64) -> f64 {
        confidence.clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// LauBridge — bidirectional bridge
// ---------------------------------------------------------------------------

const DEFAULT_QUEUE_CAPACITY: usize = 1024;

/// Bidirectional bridge between external systems and PLATO rooms.
#[derive(Debug)]
pub struct LauBridge {
    incoming: Vec<LauMessage>,
    outgoing: Vec<LauMessage>,
    capacity: usize,
}

impl LauBridge {
    /// Create a new bridge with the default queue capacity.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_QUEUE_CAPACITY)
    }

    /// Create a bridge with a custom per-queue capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            incoming: Vec::new(),
            outgoing: Vec::new(),
            capacity,
        }
    }

    /// External event → internal PLATO event.
    pub fn port_in(&mut self, msg: LauMessage) {
        if self.incoming.len() < self.capacity {
            self.incoming.push(msg);
        }
    }

    /// Internal PLATO event → external game event.
    pub fn port_out(&mut self, msg: LauMessage) {
        if self.outgoing.len() < self.capacity {
            self.outgoing.push(msg);
        }
    }

    /// Drain and return all pending outgoing messages.
    pub fn flush_outgoing(&mut self) -> Vec<LauMessage> {
        std::mem::take(&mut self.outgoing)
    }

    /// Drain and return all pending incoming messages.
    pub fn flush_incoming(&mut self) -> Vec<LauMessage> {
        std::mem::take(&mut self.incoming)
    }

    /// Current number of queued incoming messages.
    pub fn incoming_len(&self) -> usize {
        self.incoming.len()
    }

    /// Current number of queued outgoing messages.
    pub fn outgoing_len(&self) -> usize {
        self.outgoing.len()
    }
}

impl Default for LauBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// VoxelChunk & GameWorld
// ---------------------------------------------------------------------------

pub const CHUNK_SIZE: usize = 16;

/// A 16×16×16 cube of voxels. Materials are stored as `u8` values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoxelChunk {
    voxels: Vec<u8>,
}

impl VoxelChunk {
    /// Create an empty chunk (all air / material 0).
    pub fn new() -> Self {
        Self {
            voxels: vec![0u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE],
        }
    }

    fn idx(x: usize, y: usize, z: usize) -> usize {
        x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE
    }

    /// Get the material at local coordinates.
    pub fn get(&self, x: usize, y: usize, z: usize) -> u8 {
        self.voxels[Self::idx(x, y, z)]
    }

    /// Set the material at local coordinates.
    pub fn set(&mut self, x: usize, y: usize, z: usize, material: u8) {
        self.voxels[Self::idx(x, y, z)] = material;
    }

    /// Raw voxel slice for serialization.
    pub fn as_slice(&self) -> &[u8] {
        &self.voxels[..]
    }
}

impl Default for VoxelChunk {
    fn default() -> Self {
        Self::new()
    }
}

/// An agent positioned in the game world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameAgent {
    pub id: String,
    pub position: (f64, f64, f64),
    pub appearance: AvatarAppearance,
    pub room_id: String,
}

/// Lightweight voxel world model with agents.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GameWorld {
    pub chunks: HashMap<(i32, i32, i32), VoxelChunk>,
    pub agents: HashMap<String, GameAgent>,
}

impl GameWorld {
    /// Create an empty world.
    pub fn new() -> Self {
        Self::default()
    }

    fn chunk_pos(x: i32, y: i32, z: i32) -> (i32, i32, i32) {
        let floor = |v: i32, s: i32| {
            if v >= 0 {
                v / s
            } else {
                (v + 1) / s - 1
            }
        };
        (floor(x, CHUNK_SIZE as i32), floor(y, CHUNK_SIZE as i32), floor(z, CHUNK_SIZE as i32))
    }

    fn local_coord(v: i32) -> usize {
        ((v % CHUNK_SIZE as i32).rem_euclid(CHUNK_SIZE as i32)) as usize
    }

    /// Place a voxel of the given material at world coordinates.
    pub fn place_voxel(&mut self, x: i32, y: i32, z: i32, material: u8) {
        let cp = Self::chunk_pos(x, y, z);
        let chunk = self.chunks.entry(cp).or_default();
        chunk.set(Self::local_coord(x), Self::local_coord(y), Self::local_coord(z), material);
    }

    /// Remove a voxel (set to 0) at world coordinates.
    pub fn remove_voxel(&mut self, x: i32, y: i32, z: i32) {
        let cp = Self::chunk_pos(x, y, z);
        if let Some(chunk) = self.chunks.get_mut(&cp) {
            chunk.set(Self::local_coord(x), Self::local_coord(y), Self::local_coord(z), 0);
        }
    }

    /// Get the material at world coordinates (0 if no chunk).
    pub fn get_voxel(&self, x: i32, y: i32, z: i32) -> u8 {
        let cp = Self::chunk_pos(x, y, z);
        self.chunks
            .get(&cp)
            .map(|c| c.get(Self::local_coord(x), Self::local_coord(y), Self::local_coord(z)))
            .unwrap_or(0)
    }

    /// Add an agent to the world at the given position.
    pub fn add_agent(&mut self, id: impl Into<String>, x: f64, y: f64, z: f64) {
        let id = id.into();
        self.agents.insert(
            id.clone(),
            GameAgent {
                id,
                position: (x, y, z),
                appearance: AvatarMapping::room_vibe_to_appearance(0.5),
                room_id: String::new(),
            },
        );
    }

    /// Move an existing agent to new coordinates.
    pub fn move_agent(&mut self, id: &str, x: f64, y: f64, z: f64) -> bool {
        if let Some(agent) = self.agents.get_mut(id) {
            agent.position = (x, y, z);
            true
        } else {
            false
        }
    }

    /// Return agents within `radius` of the given point.
    pub fn nearby_agents(&self, x: f64, y: f64, z: f64, radius: f64) -> Vec<&GameAgent> {
        let r2 = radius * radius;
        self.agents
            .values()
            .filter(|a| {
                let dx = a.position.0 - x;
                let dy = a.position.1 - y;
                let dz = a.position.2 - z;
                dx * dx + dy * dy + dz * dz <= r2
            })
            .collect()
    }

    /// Serialize a chunk to raw bytes for network transport.
    pub fn serialize_chunk(&self, chunk_pos: (i32, i32, i32)) -> Option<Vec<u8>> {
        self.chunks.get(&chunk_pos).map(|c| c.as_slice().to_vec())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- LauMessage serialization round-trips --

    #[test]
    fn action_round_trip() {
        let msg = LauMessage::Action {
            agent_id: "a1".into(),
            action: "shoot".into(),
            params: {
                let mut m = HashMap::new();
                m.insert("power".into(), 42.0);
                m
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: LauMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn observe_round_trip() {
        let msg = LauMessage::Observe {
            agent_id: "b".into(),
            sensor: "proximity".into(),
            value: 3.14,
            confidence: 0.9,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: LauMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn speak_round_trip() {
        let msg = LauMessage::Speak {
            agent_id: "c".into(),
            text: "hello world".into(),
            emotion: 0.6,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: LauMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn move_round_trip() {
        let msg = LauMessage::Move {
            agent_id: "d".into(),
            x: 1.0,
            y: 2.0,
            z: 3.0,
            facing: 90.0,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: LauMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn interact_round_trip() {
        let msg = LauMessage::Interact {
            agent_id: "e".into(),
            target_id: "door_42".into(),
            kind: "open".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: LauMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn state_update_round_trip() {
        let msg = LauMessage::StateUpdate {
            room_id: "room_1".into(),
            vibe: 0.75,
            population: 5,
            events: vec!["agent_joined".into()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: LauMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    // -- AvatarMapping --

    #[test]
    fn vibe_zero_is_blueish() {
        let a = AvatarMapping::room_vibe_to_appearance(0.0);
        assert!(a.color.0 < a.color.2); // R < B
        assert_eq!(a.glow, 0.0);
    }

    #[test]
    fn vibe_one_is_warm() {
        let a = AvatarMapping::room_vibe_to_appearance(1.0);
        assert!(a.color.0 > a.color.2); // R > B
        assert_eq!(a.glow, 1.0);
    }

    #[test]
    fn vibe_clamps() {
        let low = AvatarMapping::room_vibe_to_appearance(-5.0);
        let high = AvatarMapping::room_vibe_to_appearance(10.0);
        assert_eq!(low.glow, 0.0);
        assert_eq!(high.glow, 1.0);
    }

    #[test]
    fn phase_animations() {
        assert_eq!(AvatarMapping::room_phase_to_animation("Gestating"), "idle");
        assert_eq!(AvatarMapping::room_phase_to_animation("Forming"), "exploring");
        assert_eq!(AvatarMapping::room_phase_to_animation("Stable"), "confident");
        assert_eq!(AvatarMapping::room_phase_to_animation("Dissolving"), "fading");
        assert_eq!(AvatarMapping::room_phase_to_animation("Custom"), "custom");
    }

    #[test]
    fn confidence_maps_to_visibility() {
        assert_eq!(AvatarMapping::confidence_to_visibility(0.0), 0.0);
        assert_eq!(AvatarMapping::confidence_to_visibility(1.0), 1.0);
        assert_eq!(AvatarMapping::confidence_to_visibility(0.5), 0.5);
        assert_eq!(AvatarMapping::confidence_to_visibility(-1.0), 0.0);
        assert_eq!(AvatarMapping::confidence_to_visibility(2.0), 1.0);
    }

    // -- LauBridge --

    #[test]
    fn bridge_port_in_and_flush() {
        let mut bridge = LauBridge::new();
        let msg = LauMessage::Speak {
            agent_id: "x".into(),
            text: "hi".into(),
            emotion: 0.5,
        };
        bridge.port_in(msg.clone());
        assert_eq!(bridge.incoming_len(), 1);
        let msgs = bridge.flush_incoming();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], msg);
        assert_eq!(bridge.incoming_len(), 0);
    }

    #[test]
    fn bridge_port_out_and_flush() {
        let mut bridge = LauBridge::new();
        bridge.port_out(LauMessage::StateUpdate {
            room_id: "r".into(),
            vibe: 0.5,
            population: 1,
            events: vec![],
        });
        assert_eq!(bridge.outgoing_len(), 1);
        let msgs = bridge.flush_outgoing();
        assert_eq!(msgs.len(), 1);
        assert!(bridge.flush_outgoing().is_empty());
    }

    #[test]
    fn bridge_capacity_limit() {
        let mut bridge = LauBridge::with_capacity(2);
        for _ in 0..5 {
            bridge.port_in(LauMessage::Speak {
                agent_id: "x".into(),
                text: "hi".into(),
                emotion: 0.5,
            });
        }
        assert_eq!(bridge.incoming_len(), 2);
    }

    #[test]
    fn bridge_default_impl() {
        let bridge = LauBridge::default();
        assert_eq!(bridge.incoming_len(), 0);
        assert_eq!(bridge.outgoing_len(), 0);
    }

    // -- VoxelChunk --

    #[test]
    fn chunk_starts_empty() {
        let c = VoxelChunk::new();
        assert_eq!(c.get(0, 0, 0), 0);
        assert_eq!(c.get(15, 15, 15), 0);
    }

    #[test]
    fn chunk_set_and_get() {
        let mut c = VoxelChunk::new();
        c.set(5, 10, 3, 42);
        assert_eq!(c.get(5, 10, 3), 42);
        assert_eq!(c.get(5, 10, 4), 0);
    }

    #[test]
    fn chunk_serialization() {
        let mut c = VoxelChunk::new();
        c.set(0, 0, 0, 7);
        let slice = c.as_slice();
        assert_eq!(slice[0], 7);
        assert_eq!(slice.len(), CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE);
    }

    // -- GameWorld --

    #[test]
    fn world_place_and_get() {
        let mut w = GameWorld::new();
        w.place_voxel(3, 7, 11, 99);
        assert_eq!(w.get_voxel(3, 7, 11), 99);
        assert_eq!(w.get_voxel(3, 7, 12), 0);
    }

    #[test]
    fn world_remove_voxel() {
        let mut w = GameWorld::new();
        w.place_voxel(1, 2, 3, 55);
        assert_eq!(w.get_voxel(1, 2, 3), 55);
        w.remove_voxel(1, 2, 3);
        assert_eq!(w.get_voxel(1, 2, 3), 0);
    }

    #[test]
    fn world_remove_nonexistent_is_noop() {
        let mut w = GameWorld::new();
        w.remove_voxel(100, 200, 300); // no chunk exists
        // shouldn't panic
    }

    #[test]
    fn world_negative_coords() {
        let mut w = GameWorld::new();
        w.place_voxel(-1, -1, -1, 77);
        assert_eq!(w.get_voxel(-1, -1, -1), 77);
    }

    #[test]
    fn world_cross_chunk_boundary() {
        let mut w = GameWorld::new();
        w.place_voxel(15, 0, 0, 1); // chunk (0,0,0)
        w.place_voxel(16, 0, 0, 2); // chunk (1,0,0)
        assert_eq!(w.get_voxel(15, 0, 0), 1);
        assert_eq!(w.get_voxel(16, 0, 0), 2);
        assert_eq!(w.chunks.len(), 2);
    }

    #[test]
    fn world_add_and_move_agent() {
        let mut w = GameWorld::new();
        w.add_agent("a1", 1.0, 2.0, 3.0);
        assert!(w.agents.contains_key("a1"));
        assert_eq!(w.agents["a1"].position, (1.0, 2.0, 3.0));

        let moved = w.move_agent("a1", 4.0, 5.0, 6.0);
        assert!(moved);
        assert_eq!(w.agents["a1"].position, (4.0, 5.0, 6.0));
    }

    #[test]
    fn world_move_nonexistent_agent() {
        let mut w = GameWorld::new();
        assert!(!w.move_agent("ghost", 0.0, 0.0, 0.0));
    }

    #[test]
    fn world_nearby_agents() {
        let mut w = GameWorld::new();
        w.add_agent("near", 1.0, 0.0, 0.0);
        w.add_agent("far", 100.0, 0.0, 0.0);
        let nearby = w.nearby_agents(0.0, 0.0, 0.0, 5.0);
        assert_eq!(nearby.len(), 1);
        assert_eq!(nearby[0].id, "near");
    }

    #[test]
    fn world_nearby_agents_boundary() {
        let mut w = GameWorld::new();
        w.add_agent("edge", 3.0, 4.0, 0.0); // distance = 5.0
        let nearby = w.nearby_agents(0.0, 0.0, 0.0, 5.0);
        assert_eq!(nearby.len(), 1);
    }

    #[test]
    fn world_serialize_chunk() {
        let mut w = GameWorld::new();
        w.place_voxel(0, 0, 0, 42);
        let cp = GameWorld::chunk_pos(0, 0, 0);
        let data = w.serialize_chunk(cp).unwrap();
        assert_eq!(data.len(), CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE);
        assert_eq!(data[0], 42);
    }

    #[test]
    fn world_serialize_missing_chunk() {
        let w = GameWorld::new();
        assert!(w.serialize_chunk((99, 99, 99)).is_none());
    }

    // -- Full integration: bridge + world --

    #[test]
    fn bridge_to_world_move() {
        let mut bridge = LauBridge::new();
        let mut world = GameWorld::new();
        world.add_agent("hero", 0.0, 0.0, 0.0);

        bridge.port_in(LauMessage::Move {
            agent_id: "hero".into(),
            x: 10.0,
            y: 20.0,
            z: 30.0,
            facing: 45.0,
        });

        for msg in bridge.flush_incoming() {
            if let LauMessage::Move { agent_id, x, y, z, .. } = msg {
                world.move_agent(&agent_id, x, y, z);
            }
        }
        assert_eq!(world.agents["hero"].position, (10.0, 20.0, 30.0));
    }

    #[test]
    fn world_to_bridge_state_update() {
        let mut bridge = LauBridge::new();
        bridge.port_out(LauMessage::StateUpdate {
            room_id: "room_1".into(),
            vibe: 0.9,
            population: 3,
            events: vec!["agent_spawned".into()],
        });
        let msgs = bridge.flush_outgoing();
        assert_eq!(msgs.len(), 1);
        let json = serde_json::to_string(&msgs[0]).unwrap();
        assert!(json.contains("room_1"));
    }
}

//! What Sentinel knows about one living entity when it decides about it: the facts its target
//! lists match against, read from the game by the host each update.

/// A position in a level.
pub type Vec3 = [f64; 3];

/// An item an entity holds, wears or carries, as the item matchers see it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemView {
    /// `minecraft:diamond_sword`.
    pub id: String,
    /// The custom name as plain text (`name:` patterns), when it has one.
    pub display_name: Option<String>,
    /// Lore lines as plain text (`lore:` patterns).
    pub lore: Vec<String>,
    /// Sharpness / Power levels for Sentinel's own damage estimate.
    pub sharpness: u32,
    pub power: u32,
}

impl ItemView {
    pub fn of(id: &str) -> Self {
        ItemView { id: id.to_owned(), ..Default::default() }
    }
    /// The Bukkit material name (`DIAMOND_SWORD`) that bare item patterns match.
    pub fn material(&self) -> String {
        self.id.split_once(':').map_or(self.id.as_str(), |(_, k)| k).to_uppercase()
    }
}

/// Game mode, for "creative and spectator players are never targets".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameMode {
    #[default]
    Survival,
    Creative,
    Adventure,
    Spectator,
}

/// The NPC side of an entity that is a Citizens NPC.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NpcInfo {
    pub id: i32,
    /// Its full name (`npc:` patterns match it).
    pub name: String,
    /// Whether it has a Sentinel trait.
    pub sentinel: bool,
}

/// One living entity (a player, a mob, an NPC body).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityView {
    /// The level-unique entity id.
    pub id: i32,
    pub uuid: u128,
    /// `minecraft:zombie`, `minecraft:player`.
    pub kind: String,
    pub dimension: String,
    pub pos: Vec3,
    pub eye: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub velocity: Vec3,
    pub alive: bool,
    pub health: f64,
    pub max_health: f64,
    /// A player's name; None for anything else.
    pub player_name: Option<String>,
    pub game_mode: GameMode,
    /// A mob's custom name as plain text.
    pub custom_name: Option<String>,
    /// Some when the entity is an NPC.
    pub npc: Option<NpcInfo>,
    pub main_hand: Option<ItemView>,
    pub off_hand: Option<ItemView>,
    /// Head, chest, legs, feet.
    pub armor: [Option<ItemView>; 4],
    /// A player's storage slots (empty for mobs).
    pub inventory: Vec<ItemView>,
    /// Permission groups (`group:` targets); filled only when a permissions bridge exists.
    pub groups: Vec<String>,
    /// A mob's current target: Some(true) when angry, Some(false) when not, None for no mob.
    pub mob_has_target: Option<bool>,
    pub invisible_effect: bool,
    pub glowing: bool,
    pub sprinting: bool,
}

impl EntityView {
    pub fn is_player(&self) -> bool {
        self.kind == "minecraft:player" && self.npc.is_none()
    }
    pub fn is_npc(&self) -> bool {
        self.npc.is_some()
    }
    pub fn distance_sq(&self, to: Vec3) -> f64 {
        dist_sq(self.pos, to)
    }
    /// What `entityname:` patterns match: the custom name, else the type name.
    pub fn entity_name(&self) -> String {
        self.custom_name.clone().unwrap_or_else(|| crate::target::bukkit_type_name(&self.kind))
    }
    /// `SentinelUtilities.isInvisible`: an invisibility effect and nothing in either hand (armor
    /// is ignored, as in Sentinel).
    pub fn is_invisible(&self) -> bool {
        self.invisible_effect && self.main_hand.is_none() && self.off_hand.is_none()
    }
    /// `isUntargetable`: dead, or a creative or spectator player.
    pub fn untargetable(&self) -> bool {
        !self.alive
            || (self.is_player() && matches!(self.game_mode, GameMode::Creative | GameMode::Spectator))
    }
}

pub fn dist_sq(a: Vec3, b: Vec3) -> f64 {
    let (x, y, z) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    x * x + y * y + z * z
}

/// `SentinelUtilities.isLookingTowards`: whether `from` (eye position, yaw, pitch) faces `to`
/// within the yaw and pitch limits, in degrees.
pub fn looking_towards(from: Vec3, yaw: f32, pitch: f32, to: Vec3, yaw_limit: f32, pitch_limit: f32) -> bool {
    let rel = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let len = (rel[0] * rel[0] + rel[1] * rel[1] + rel[2] * rel[2]).sqrt();
    if len == 0.0 {
        return true;
    }
    let rel = [rel[0] / len, rel[1] / len, rel[2] / len];
    let yaw = normalize_yaw(yaw);
    let yaw_help = yaw_of(rel);
    if !((yaw_help - yaw).abs() < yaw_limit
        || (yaw_help + 360.0 - yaw).abs() < yaw_limit
        || (yaw + 360.0 - yaw_help).abs() < yaw_limit)
    {
        return false;
    }
    (pitch_of(rel) - pitch).abs() < pitch_limit
}

pub fn normalize_yaw(yaw: f32) -> f32 {
    let y = yaw % 360.0;
    if y < 0.0 { y + 360.0 } else { y }
}

/// Minecraft yaw of a direction (0 = +z, 90 = -x).
pub fn yaw_of(dir: Vec3) -> f32 {
    let yaw = (-dir[0]).atan2(dir[2]).to_degrees() as f32;
    normalize_yaw(yaw)
}

pub fn pitch_of(dir: Vec3) -> f32 {
    let xz = (dir[0] * dir[0] + dir[2] * dir[2]).sqrt();
    (-dir[1]).atan2(xz).to_degrees() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facing() {
        // Yaw 0 looks along +z.
        assert!(looking_towards([0.0; 3], 0.0, 0.0, [0.0, 0.0, 5.0], 45.0, 45.0));
        assert!(!looking_towards([0.0; 3], 180.0, 0.0, [0.0, 0.0, 5.0], 45.0, 45.0));
        assert!(looking_towards([0.0; 3], 90.0, 0.0, [-5.0, 0.0, 0.0], 45.0, 45.0));
    }

    #[test]
    fn invisible_only_with_empty_hands() {
        let mut e = EntityView { invisible_effect: true, alive: true, ..Default::default() };
        assert!(e.is_invisible());
        e.main_hand = Some(ItemView::of("minecraft:stick"));
        assert!(!e.is_invisible());
    }
}

//! Target lists (`SentinelTargetList`) and target labels (`SentinelTargetLabel`): what
//! `addtarget`, `addignore` and `addavoid` add, how they are saved, and whether an entity matches.
//!
//! Translated from Sentinel (MIT, see LICENSE). Name patterns are regular expressions matched
//! case-insensitively anywhere in the name (`.*<pattern>.*`), as in Sentinel: `npc:3` ignores
//! every NPC whose name contains a `3`. Rust's `regex` syntax stands in for Java's; the two agree
//! on everything but look-around and back-references.

use crate::node::Node;
use crate::target::{self, NPCS, OWNER};
use crate::view::{EntityView, ItemView};
use regex::Regex;
use std::collections::HashMap;
use std::sync::Mutex;

/// The prefixes whose values are regular expressions.
const REGEX_PREFIXES: &[&str] = &["player", "npc", "entityname", "helditem", "offhand", "equipped", "in_inventory"];
/// The prefixes Sentinel itself knows (others belong to integrations: `squad:`, `team:` ...).
const CORE_PREFIXES: &[&str] = &[
    "player", "npc", "entityname", "helditem", "offhand", "equipped", "in_inventory", "group", "status",
    "event", "multi", "allinone",
];
/// The prefixes of Sentinel's own integrations (`SentinelIntegration.getTargetPrefixes` of the
/// ones that need no other plugin). Towny, Factions, War, SimpleClans, SimplePets and CrackShot
/// register theirs only when that plugin runs, so here they are unknown prefixes, as on a server
/// without them.
const OTHER_PREFIXES: &[&str] = &[
    "healthabove", "healthbelow", "npc_owned_by", "permission", "potion", "sbscoreabove", "sbscorebelow", "sbteam",
    "squad", "uuid",
];
/// Prefixes whose value is lower-cased when the label is read (`autoLowercasePrefixes` and
/// the integrations whose `shouldLowerCaseValue` is true).
const LOWERCASE_PREFIXES: &[&str] = &["status", "event", "squad", "uuid"];
/// `SentinelPlugin.validEventTargets`.
pub const EVENT_TARGETS: &[&str] = &["pvp", "pve", "pvnpc", "pvsentinel", "eve", "npcvnpc", "guarded_fight", "message", "pv", "ev"];

fn regex_cache() -> &'static Mutex<HashMap<String, Option<Regex>>> {
    static C: std::sync::OnceLock<Mutex<HashMap<String, Option<Regex>>>> = std::sync::OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `SentinelUtilities.isRegexTarget`: `.*<pattern>.*`, case-insensitive. A pattern that is no
/// valid expression never matches.
pub fn regex_matches(name: &str, pattern: &str) -> bool {
    let mut cache = regex_cache().lock().unwrap_or_else(|e| e.into_inner());
    let re = cache
        .entry(pattern.to_owned())
        .or_insert_with(|| Regex::new(&format!("(?i)^.*(?:{pattern}).*$")).ok());
    re.as_ref().is_some_and(|r| r.is_match(name))
}

pub fn valid_regex(pattern: &str) -> bool {
    Regex::new(&format!("(?i)^.*(?:{pattern}).*$")).is_ok()
}

fn regex_target<'a>(name: &str, patterns: &'a [String]) -> Option<&'a String> {
    patterns.iter().find(|p| regex_matches(name, p))
}

/// `SentinelUtilities.getItemTarget`: `air`, `name:<re>`, `lore:<re>` or a material pattern.
fn item_target<'a>(item: Option<&ItemView>, targets: &'a [String]) -> Option<&'a String> {
    let Some(item) = item.filter(|i| i.id != "minecraft:air") else {
        return targets.iter().find(|t| t.as_str() == "air");
    };
    let material = item.material();
    targets.iter().find(|t| {
        if let Some(m) = t.strip_prefix("name:") {
            item.display_name.as_deref().is_some_and(|n| regex_matches(n, m))
        } else if let Some(m) = t.strip_prefix("lore:") {
            item.lore.iter().any(|l| regex_matches(l, m))
        } else {
            regex_matches(&material, t)
        }
    })
}

/// What a list needs to know about the NPC that asks (`isTarget(entity, sentinel)`).
#[derive(Clone, Debug, Default)]
pub struct Asker {
    /// The NPC's owner (`OWNER` targets).
    pub owner: Option<u128>,
    /// Squad names of other Sentinels by entity id, for `squad:` targets.
    pub squads: HashMap<i32, String>,
}

/// A target list: `allTargets`, `allIgnores` or `allAvoids`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TargetList {
    /// Canonical target type names (`PLAYER`, `NPC`, `OWNER`, `MONSTER`).
    pub targets: Vec<String>,
    pub by_player_name: Vec<String>,
    pub by_npc_name: Vec<String>,
    pub by_entity_name: Vec<String>,
    pub by_held_item: Vec<String>,
    pub by_offhand_item: Vec<String>,
    pub by_equipped_item: Vec<String>,
    pub by_inventory_item: Vec<String>,
    pub by_group: Vec<String>,
    pub by_event: Vec<String>,
    pub by_status: Vec<String>,
    /// `prefix:value` of integration targets.
    pub by_other: Vec<String>,
    pub by_all_in_one: Vec<TargetList>,
    pub by_multiple: Vec<TargetList>,
}

/// The saved keys (`@Persist` names) of the string lists, in field order.
const STRING_KEYS: &[&str] = &[
    "targets", "byPlayerName", "byNpcName", "byEntityName", "byHeldItem", "byOffhandItem", "byEquippedItem",
    "byInventoryItem", "byGroup", "byEvent", "byStatus", "byOther",
];

impl TargetList {
    fn lists(&self) -> [&Vec<String>; 12] {
        [
            &self.targets, &self.by_player_name, &self.by_npc_name, &self.by_entity_name, &self.by_held_item,
            &self.by_offhand_item, &self.by_equipped_item, &self.by_inventory_item, &self.by_group,
            &self.by_event, &self.by_status, &self.by_other,
        ]
    }
    fn lists_mut(&mut self) -> [&mut Vec<String>; 12] {
        [
            &mut self.targets, &mut self.by_player_name, &mut self.by_npc_name, &mut self.by_entity_name,
            &mut self.by_held_item, &mut self.by_offhand_item, &mut self.by_equipped_item,
            &mut self.by_inventory_item, &mut self.by_group, &mut self.by_event, &mut self.by_status,
            &mut self.by_other,
        ]
    }

    /// Reads a saved list (`PersistenceLoader.load` + `init`).
    pub fn load(node: &Node) -> Self {
        let mut list = TargetList::default();
        for (key, field) in STRING_KEYS.iter().zip(list.lists_mut()) {
            *field = node.strings(key);
        }
        list.by_all_in_one = sub_lists(node.get("byAllInOne"));
        list.by_multiple = sub_lists(node.get("byMultiple"));
        list.init();
        list
    }

    /// `init`: canonical target names, and the old `message,` event spelling.
    pub fn init(&mut self) {
        self.recalculate();
        for e in self.by_event.iter_mut() {
            if let Some(rest) = e.strip_prefix("message,") {
                *e = format!("message:{rest}");
            }
        }
    }

    /// `recalculateTargetsCache`: target names become canonical and unique; unknown ones go.
    pub fn recalculate(&mut self) {
        let mut seen: Vec<String> = Vec::new();
        for t in &self.targets {
            if let Some(c) = target::canonical(t) {
                if !seen.iter().any(|s| s == c) {
                    seen.push(c.to_owned());
                }
            }
        }
        self.targets = seen;
    }

    /// Writes the list as Citizens saves it; empty lists are left out.
    pub fn save(&self) -> Node {
        let mut node = Node::map();
        for (key, field) in STRING_KEYS.iter().zip(self.lists()) {
            if !field.is_empty() {
                node.set(key, Node::string_list(field));
            }
        }
        for (key, subs) in [("byAllInOne", &self.by_all_in_one), ("byMultiple", &self.by_multiple)] {
            if !subs.is_empty() {
                node.set(key, Node::List(subs.iter().map(TargetList::save).collect()));
            }
        }
        node
    }

    pub fn total(&self) -> usize {
        self.lists().iter().map(|l| l.len()).sum::<usize>() + self.by_all_in_one.len() + self.by_multiple.len()
    }

    fn has_type(&self, name: &str) -> bool {
        self.targets.iter().any(|t| t == name)
    }

    /// `isTarget(entity, sentinel)`: the list matches the entity.
    pub fn is_target(&self, e: &EntityView, asker: &Asker) -> bool {
        if self.has_type(OWNER) && asker.owner.is_some_and(|o| o == e.uuid) {
            return true;
        }
        self.is_target_no_owner(e, asker)
    }

    fn is_target_no_owner(&self, e: &EntityView, asker: &Asker) -> bool {
        if self.total() == 0 {
            return false;
        }
        if item_target(e.main_hand.as_ref(), &self.by_held_item).is_some()
            || item_target(e.off_hand.as_ref(), &self.by_offhand_item).is_some()
        {
            return true;
        }
        if !self.by_equipped_item.is_empty()
            && e.armor.iter().any(|a| item_target(a.as_ref(), &self.by_equipped_item).is_some())
        {
            return true;
        }
        if !self.by_inventory_item.is_empty()
            && e.inventory.iter().any(|i| item_target(Some(i), &self.by_inventory_item).is_some())
        {
            return true;
        }
        if self.by_other.iter().any(|o| other_target(o, e, asker)) {
            return true;
        }
        for all in &self.by_all_in_one {
            let mut sub = all.clone();
            while sub.take_match(e, asker) {}
            if sub.total() == 0 {
                return true;
            }
        }
        // An NPC is matched by type `NPC` or by name and nothing else.
        if let Some(npc) = &e.npc {
            return self.has_type(NPCS) || regex_target(&npc.name, &self.by_npc_name).is_some();
        }
        if e.is_player() {
            if let Some(name) = &e.player_name {
                if regex_target(name, &self.by_player_name).is_some() {
                    return true;
                }
            }
            if self.by_group.iter().any(|g| e.groups.iter().any(|h| h.eq_ignore_ascii_case(g))) {
                return true;
            }
        } else if regex_target(&e.entity_name(), &self.by_entity_name).is_some() {
            return true;
        }
        match e.mob_has_target {
            Some(true) if self.by_status.iter().any(|s| s == "angry") => return true,
            Some(false) if self.by_status.iter().any(|s| s == "passive") => return true,
            _ => {}
        }
        target::types_of_kind(&e.kind).any(|t| self.has_type(t))
    }

    /// `ifIsTargetDeleteTarget`: takes the first entry that matches out of the list (for
    /// `multi:` and `allinone:` lists, which are complete once empty).
    pub fn take_match(&mut self, e: &EntityView, asker: &Asker) -> bool {
        if self.total() == 0 {
            return false;
        }
        fn take(list: &mut Vec<String>, hit: Option<String>) -> bool {
            match hit {
                Some(h) => {
                    if let Some(i) = list.iter().position(|x| *x == h) {
                        list.remove(i);
                    }
                    true
                }
                None => false,
            }
        }
        let hit = item_target(e.main_hand.as_ref(), &self.by_held_item).cloned();
        if take(&mut self.by_held_item, hit) {
            return true;
        }
        let hit = item_target(e.off_hand.as_ref(), &self.by_offhand_item).cloned();
        if take(&mut self.by_offhand_item, hit) {
            return true;
        }
        for a in &e.armor {
            let hit = item_target(a.as_ref(), &self.by_equipped_item).cloned();
            if take(&mut self.by_equipped_item, hit) {
                return true;
            }
        }
        for i in &e.inventory {
            let hit = item_target(Some(i), &self.by_inventory_item).cloned();
            if take(&mut self.by_inventory_item, hit) {
                return true;
            }
        }
        let hit = self.by_other.iter().find(|o| other_target(o, e, asker)).cloned();
        if take(&mut self.by_other, hit) {
            return true;
        }
        if let Some(i) = self.by_all_in_one.iter().position(|all| {
            let mut sub = all.clone();
            while sub.take_match(e, asker) {}
            sub.total() == 0
        }) {
            self.by_all_in_one.remove(i);
            return true;
        }
        if let Some(npc) = &e.npc {
            if self.has_type(NPCS) {
                self.targets.retain(|t| t != NPCS);
                return true;
            }
            let hit = regex_target(&npc.name, &self.by_npc_name).cloned();
            return take(&mut self.by_npc_name, hit);
        }
        if e.is_player() {
            let hit = e.player_name.as_deref().and_then(|n| regex_target(n, &self.by_player_name)).cloned();
            if take(&mut self.by_player_name, hit) {
                return true;
            }
            let hit = self.by_group.iter().find(|g| e.groups.iter().any(|h| h.eq_ignore_ascii_case(g))).cloned();
            if take(&mut self.by_group, hit) {
                return true;
            }
        } else {
            let hit = regex_target(&e.entity_name(), &self.by_entity_name).cloned();
            if take(&mut self.by_entity_name, hit) {
                return true;
            }
        }
        for status in ["angry", "passive"] {
            let applies = match status {
                "angry" => e.mob_has_target == Some(true),
                _ => e.mob_has_target == Some(false),
            };
            if applies && self.by_status.iter().any(|s| s == status) {
                self.by_status.retain(|s| s != status);
                return true;
            }
        }
        if let Some(t) = target::types_of_kind(&e.kind).find(|t| self.has_type(t)) {
            self.targets.retain(|x| x != t);
            return true;
        }
        false
    }

    /// `toComboString` with `separator` between entries (`, ` for display, `,` for `multi:`,
    /// `|` for `allinone:`).
    pub fn combo_string(&self, separator: &str) -> String {
        let prefixes = [
            None, Some("player"), Some("npc"), Some("entityname"), Some("helditem"), Some("offhand"),
            Some("equipped"), Some("in_inventory"), Some("group"), Some("event"), Some("status"), None,
        ];
        let mut parts = Vec::new();
        for (field, prefix) in self.lists().into_iter().zip(prefixes) {
            for v in field {
                parts.push(match prefix {
                    Some(p) => format!("{p}:{v}"),
                    None => v.clone(),
                });
            }
        }
        for all in &self.by_all_in_one {
            parts.push(format!("allinone:{}", all.combo_string("|")));
        }
        for multi in &self.by_multiple {
            parts.push(format!("multi:{}", multi.combo_string(",")));
        }
        parts.join(separator)
    }

    /// `getTargetRemovableStrings`: what `removetarget` accepts, lower case.
    pub fn removable_strings(&self) -> Vec<String> {
        let mut out = Vec::new();
        let prefixes = [
            None, Some("player"), Some("npc"), Some("entityname"), Some("helditem"), Some("offhand"),
            Some("equipped"), Some("in_inventory"), Some("group"), Some("event"), Some("status"), None,
        ];
        for (field, prefix) in self.lists().into_iter().zip(prefixes) {
            for v in field {
                out.push(match prefix {
                    Some(p) => format!("{p}:{}", v.to_lowercase()),
                    None => v.to_lowercase(),
                });
            }
        }
        for all in &self.by_all_in_one {
            out.push(format!("allinone:{}", all.combo_string("|").to_lowercase().replace(' ', "")));
        }
        for multi in &self.by_multiple {
            out.push(format!("multi:{}", multi.combo_string(",").to_lowercase().replace(' ', "")));
        }
        out
    }

    /// `isEventTarget(sentinel, chat)`: a chat message holds one of the `message:` texts.
    pub fn is_message_target(&self, message: &str) -> bool {
        let lower = message.to_lowercase();
        self.by_event
            .iter()
            .filter_map(|e| e.strip_prefix("message:"))
            .any(|m| lower.contains(&m.to_lowercase()))
    }

    /// `isReverseEventTarget`: `guarded_fight` and the guarded entity attacked someone.
    pub fn is_reverse_event_target(&self, damager_uuid: u128, guarding: Option<u128>) -> bool {
        self.by_event.iter().any(|e| e == "guarded_fight") && guarding == Some(damager_uuid)
    }

    /// `isEventTarget(damage event)`: whether a hit of `damager` on `victim` makes the damager a
    /// target (`pvp`, `pve`, `eve`, `pvnpc`, `pvsentinel`, `npcvnpc`, `pv:<label>`, `ev:<label>`).
    pub fn is_damage_event_target(&self, damager: &EntityView, victim: &EntityView, asker: &Asker) -> bool {
        if damager.id == victim.id && damager.dimension == victim.dimension {
            return false;
        }
        let damager_npc = damager.is_npc();
        let damager_player = damager.kind == "minecraft:player" && !damager_npc;
        let victim_player = victim.kind == "minecraft:player";
        for evt in &self.by_event {
            let hit = match evt.as_str() {
                "pvp" => victim_player && damager_player && !victim.is_npc(),
                "pve" => !victim_player && damager_player,
                "eve" => !damager_player && !damager_npc && !victim_player,
                "pvnpc" => damager_player && victim.is_npc(),
                "pvsentinel" => damager_player && victim.npc.as_ref().is_some_and(|n| n.sentinel),
                "npcvnpc" => damager_npc && victim.is_npc(),
                other => match other.split_once(':') {
                    Some(("pv", label)) if !damager_npc => {
                        damager_player && label_list(label).is_some_and(|l| l.is_target(victim, asker))
                    }
                    Some(("ev", label)) if !damager_npc => {
                        label_list(label).is_some_and(|l| l.is_target(victim, asker))
                    }
                    _ => false,
                },
            };
            if hit {
                return true;
            }
        }
        false
    }
}

fn label_list(label: &str) -> Option<TargetList> {
    let l = TargetLabel::parse(label);
    if !l.is_valid_target() {
        return None;
    }
    let mut list = TargetList::default();
    l.add_to(&mut list);
    Some(list)
}

fn sub_lists(node: Option<&Node>) -> Vec<TargetList> {
    match node {
        Some(Node::List(items)) => items.iter().map(TargetList::load).collect(),
        // Citizens' object lists as numbered keys (`'0': {...}`).
        Some(Node::Map(entries)) => entries.iter().map(|(_, n)| TargetList::load(n)).collect(),
        _ => Vec::new(),
    }
}

/// The integration targets this port can answer: `squad:<name>` (another Sentinel's squad),
/// `uuid:<uuid>`, `healthabove:<percent>` and `healthbelow:<percent>`. `permission:`, `potion:`,
/// `sbteam:`, `sbscoreabove:`, `sbscorebelow:` and `npc_owned_by:` are valid prefixes but never
/// match until the host can tell (see GAPS.md).
fn other_target(entry: &str, e: &EntityView, asker: &Asker) -> bool {
    let Some((prefix, value)) = entry.split_once(':') else {
        return false;
    };
    match prefix {
        // `SentinelSquads`: the entity is an NPC with a Sentinel trait in that squad.
        "squad" => asker.squads.get(&e.id).is_some_and(|s| s.eq_ignore_ascii_case(value)),
        // `SentinelUUID`: `UUID.fromString` (five dash-separated groups) equals the entity's.
        "uuid" => value.matches('-').count() == 4 && parse_uuid(value).is_some_and(|u| uuid_eq(u, e.uuid)),
        // `SentinelHealth`: health / max health against a percentage.
        "healthabove" | "healthbelow" => match value.trim().parse::<f64>() {
            Ok(pct) => {
                let ratio = e.health / e.max_health;
                if prefix == "healthabove" {
                    ratio > pct * 0.01
                } else {
                    ratio < pct * 0.01
                }
            }
            Err(_) => false,
        },
        _ => false,
    }
}

/// `SentinelUtilities.uuidEquals`: equal ignoring the UUID version nibble (an NPC's UUID is
/// version 4, its entity's version 2).
pub fn uuid_eq(a: u128, b: u128) -> bool {
    (a ^ b) & !(0x7000u128 << 64) == 0
}

pub fn parse_uuid(s: &str) -> Option<u128> {
    let hex: String = s.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    u128::from_str_radix(&hex, 16).ok()
}

pub fn format_uuid(u: u128) -> String {
    let h = format!("{u:032x}");
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// A parsed `addtarget` argument (`SentinelTargetLabel`).
#[derive(Clone, Debug, PartialEq)]
pub struct TargetLabel {
    pub prefix: Option<String>,
    pub value: String,
}

impl TargetLabel {
    pub fn parse(label: &str) -> Self {
        match label.find(':') {
            Some(i) => {
                let prefix = label[..i].to_lowercase();
                let mut value = translate_colors(&label[i + 1..]);
                if LOWERCASE_PREFIXES.contains(&prefix.as_str()) {
                    value = value.to_lowercase();
                }
                TargetLabel { prefix: Some(prefix), value }
            }
            None => {
                let upper = label.to_uppercase();
                let value = target::canonical(&upper).map(str::to_owned).unwrap_or(upper);
                TargetLabel { prefix: None, value }
            }
        }
    }

    pub fn is_regex(&self) -> bool {
        self.prefix.as_deref().is_some_and(|p| REGEX_PREFIXES.contains(&p))
    }

    /// `isValidRegex`.
    pub fn is_valid_regex(&self) -> bool {
        !self.is_regex() || valid_regex(&self.value)
    }

    /// `isValidTarget`: a known type, any prefixed value, or a known event.
    pub fn is_valid_target(&self) -> bool {
        match self.prefix.as_deref() {
            Some("event") => {
                let head = self.value.split(':').next().unwrap_or("");
                EVENT_TARGETS.contains(&head)
            }
            Some(_) => true,
            None => target::canonical(&self.value).is_some(),
        }
    }

    pub fn is_valid_prefix(&self) -> bool {
        match self.prefix.as_deref() {
            None => true,
            Some(p) => CORE_PREFIXES.contains(&p) || OTHER_PREFIXES.contains(&p),
        }
    }

    pub fn is_valid_multi(&self) -> bool {
        match self.prefix.as_deref() {
            Some("multi") => self.multi(",").total() > 0,
            Some("allinone") => self.multi("|").total() > 0,
            _ => true,
        }
    }

    fn multi(&self, splitter: &str) -> TargetList {
        let mut list = TargetList::default();
        for part in self.value.split(splitter) {
            TargetLabel::parse(part).add_unchecked(&mut list);
        }
        list.recalculate();
        list
    }

    fn field<'a>(&self, list: &'a mut TargetList) -> Option<&'a mut Vec<String>> {
        Some(match self.prefix.as_deref() {
            None => &mut list.targets,
            Some("player") => &mut list.by_player_name,
            Some("npc") => &mut list.by_npc_name,
            Some("entityname") => &mut list.by_entity_name,
            Some("helditem") => &mut list.by_held_item,
            Some("offhand") => &mut list.by_offhand_item,
            Some("equipped") => &mut list.by_equipped_item,
            Some("in_inventory") => &mut list.by_inventory_item,
            Some("group") => &mut list.by_group,
            Some("event") => &mut list.by_event,
            Some("status") => &mut list.by_status,
            Some("multi") | Some("allinone") => return None,
            Some(_) => &mut list.by_other,
        })
    }

    /// The text stored in the list: the value, or `prefix:value` for integration targets.
    fn addable(&self) -> String {
        match self.prefix.as_deref() {
            Some(p) if !CORE_PREFIXES.contains(&p) => format!("{p}:{}", self.value),
            _ => self.value.clone(),
        }
    }

    fn add_unchecked(&self, list: &mut TargetList) {
        let addable = self.addable();
        if let Some(f) = self.field(list) {
            f.push(addable);
        }
    }

    /// `addToList`: false when it was already there.
    pub fn add_to(&self, list: &mut TargetList) -> bool {
        match self.prefix.as_deref() {
            Some("multi") => {
                list.by_multiple.push(self.multi(","));
                return true;
            }
            Some("allinone") => {
                list.by_all_in_one.push(self.multi("|"));
                return true;
            }
            _ => {}
        }
        let addable = self.addable();
        let Some(field) = self.field(list) else {
            return false;
        };
        if field.contains(&addable) {
            return false;
        }
        field.push(addable);
        if self.prefix.is_none() {
            list.recalculate();
        }
        true
    }

    /// `removeFromList`: `multi:<n>` / `allinone:<n>` remove by index.
    pub fn remove_from(&self, list: &mut TargetList) -> bool {
        match self.prefix.as_deref() {
            Some(p @ ("multi" | "allinone")) => {
                let subs = if p == "multi" { &mut list.by_multiple } else { &mut list.by_all_in_one };
                match self.value.parse::<usize>() {
                    Ok(i) if i < subs.len() => {
                        subs.remove(i);
                        true
                    }
                    _ => false,
                }
            }
            _ => {
                let addable = self.addable();
                let Some(field) = self.field(list) else {
                    return false;
                };
                match field.iter().position(|x| *x == addable) {
                    Some(i) => {
                        field.remove(i);
                        true
                    }
                    None => false,
                }
            }
        }
    }
}

/// `ChatColor.translateAlternateColorCodes('&', ...)`: `&a` becomes `§a`.
fn translate_colors(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&' && i + 1 < chars.len() && "0123456789AaBbCcDdEeFfKkLlMmNnOoRrXx".contains(chars[i + 1]) {
            out.push('\u{a7}');
            out.push(chars[i + 1].to_ascii_lowercase());
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::NpcInfo;

    fn npc(id: i32, name: &str) -> EntityView {
        EntityView {
            id,
            kind: "minecraft:player".into(),
            alive: true,
            npc: Some(NpcInfo { id, name: name.into(), sentinel: false }),
            ..Default::default()
        }
    }

    fn player(name: &str) -> EntityView {
        EntityView { id: 99, kind: "minecraft:player".into(), alive: true, player_name: Some(name.into()), ..Default::default() }
    }

    #[test]
    fn addignore_npc_by_name_pattern() {
        let mut ignores = TargetList::default();
        assert!(TargetLabel::parse("npc:3").add_to(&mut ignores));
        assert!(!TargetLabel::parse("npc:3").add_to(&mut ignores));
        assert_eq!(ignores.by_npc_name, vec!["3"]);
        let asker = Asker::default();
        assert!(ignores.is_target(&npc(1, "3"), &asker));
        // Sentinel's pattern is `.*3.*`: any name with a 3 in it.
        assert!(ignores.is_target(&npc(2, "Guard 13"), &asker));
        assert!(!ignores.is_target(&npc(3, "Portal"), &asker));
        // NPC names never match players.
        assert!(!ignores.is_target(&player("x3"), &asker));
    }

    #[test]
    fn npc_type_matches_every_npc_but_no_player() {
        let mut targets = TargetList::default();
        TargetLabel::parse("npcs").add_to(&mut targets);
        assert_eq!(targets.targets, vec!["NPC"]);
        assert!(targets.is_target(&npc(1, "Bob"), &Asker::default()));
        assert!(!targets.is_target(&player("Bob"), &Asker::default()));
    }

    #[test]
    fn owner_needs_the_asker() {
        let mut ignores = TargetList::default();
        TargetLabel::parse("owner").add_to(&mut ignores);
        let mut p = player("Alice");
        p.uuid = 7;
        assert!(!ignores.is_target(&p, &Asker::default()));
        assert!(ignores.is_target(&p, &Asker { owner: Some(7), ..Default::default() }));
    }

    #[test]
    fn types_items_and_status() {
        let mut list = TargetList::default();
        TargetLabel::parse("monsters").add_to(&mut list);
        TargetLabel::parse("helditem:diamond_sword").add_to(&mut list);
        let zombie = EntityView { kind: "minecraft:zombie".into(), alive: true, ..Default::default() };
        let cow = EntityView { kind: "minecraft:cow".into(), alive: true, ..Default::default() };
        let asker = Asker::default();
        assert!(list.is_target(&zombie, &asker));
        assert!(!list.is_target(&cow, &asker));
        let mut armed = player("Bob");
        armed.main_hand = Some(ItemView::of("minecraft:diamond_sword"));
        assert!(list.is_target(&armed, &asker));
        let mut l2 = TargetList::default();
        TargetLabel::parse("status:angry").add_to(&mut l2);
        let mut angry = cow.clone();
        angry.mob_has_target = Some(true);
        assert!(l2.is_target(&angry, &asker));
    }

    #[test]
    fn saves_like_citizens() {
        let mut list = TargetList::default();
        TargetLabel::parse("owner").add_to(&mut list);
        TargetLabel::parse("npc:16").add_to(&mut list);
        let yaml = crate::node::to_yaml(&list.save(), 0);
        assert_eq!(yaml, "targets:\n- OWNER\nbyNpcName:\n- '16'\n");
        let back = TargetList::load(&crate::node::parse_yaml(&yaml).unwrap());
        assert_eq!(back, list);
    }

    #[test]
    fn multi_and_allinone() {
        let mut list = TargetList::default();
        assert!(TargetLabel::parse("allinone:player:bob|helditem:stick").add_to(&mut list));
        let asker = Asker::default();
        let mut bob = player("Bob");
        assert!(!list.is_target(&bob, &asker));
        bob.main_hand = Some(ItemView::of("minecraft:stick"));
        assert!(list.is_target(&bob, &asker));
        assert_eq!(list.removable_strings(), vec!["allinone:player:bob|helditem:stick"]);
        assert!(TargetLabel::parse("allinone:0").remove_from(&mut list));
        assert_eq!(list.total(), 0);
    }

    #[test]
    fn events() {
        let mut list = TargetList::default();
        TargetLabel::parse("event:pvp").add_to(&mut list);
        TargetLabel::parse("event:message:hello").add_to(&mut list);
        let a = player("A");
        let mut b = player("B");
        b.id = 100;
        assert!(list.is_damage_event_target(&a, &b, &Asker::default()));
        assert!(!list.is_damage_event_target(&a, &npc(5, "n"), &Asker::default()));
        assert!(list.is_message_target("well HELLO there"));
        assert!(TargetLabel::parse("event:pvp").is_valid_target());
        assert!(!TargetLabel::parse("event:nope").is_valid_target());
    }
}

//! Moza devices, told apart by their USB product name (Moza does not publish its product ids;
//! the names are the ones Boxflat recognises).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A wheel base (R3 to R21): steering, force feedback, pedals and accessories plugged in.
    Base,
    /// A pedal set on its own USB (SR-P, CRP).
    Pedals,
    /// The HBP handbrake on its own USB.
    Handbrake,
    /// Shifters, hubs, stalks, dashboards: not supported yet.
    Other,
}

pub const VENDOR: u16 = 0x346e;

/// The kind of device from its USB product name, e.g. "Gudsen MOZA R9 Base".
pub fn kind(product: &str) -> Kind {
    let p = product.to_lowercase();
    let words: Vec<&str> = p.split_whitespace().collect();
    let base_model = words
        .iter()
        .any(|w| w.len() >= 2 && w.starts_with('r') && w[1..].chars().all(|c| c.is_ascii_digit()));
    if base_model && (p.contains("base") || p.contains("racing wheel and pedals")) {
        Kind::Base
    } else if p.contains("pedals") && ["srp", "sr-p", "crp"].iter().any(|m| p.contains(m)) {
        Kind::Pedals
    } else if p.contains("handbrake") {
        Kind::Handbrake
    } else {
        Kind::Other
    }
}

/// The name to show: the product name without the maker's company name.
pub fn display_name(product: &str) -> String {
    let p = product.trim();
    let p = p.strip_prefix("Gudsen ").unwrap_or(p);
    if p.to_lowercase().starts_with("moza") {
        p.to_string()
    } else {
        format!("MOZA {p}")
    }
}

/// A catalog id, e.g. `moza/r9-base`.
pub fn model_id(product: &str) -> String {
    let name = display_name(product).to_lowercase();
    let slug: Vec<&str> = name.split_whitespace().skip(1).collect();
    format!("moza/{}", slug.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_from_names() {
        assert_eq!(kind("Gudsen MOZA R9 Base"), Kind::Base);
        assert_eq!(kind("Gudsen R5 Base"), Kind::Base);
        assert_eq!(kind("Gudsen MOZA R12 Pro Base"), Kind::Base);
        assert_eq!(kind("Gudsen MOZA R3 Racing Wheel and Pedals"), Kind::Base);
        assert_eq!(kind("Gudsen MOZA SR-P Pedals"), Kind::Pedals);
        assert_eq!(kind("Gudsen MOZA CRP2 Pedals"), Kind::Pedals);
        assert_eq!(kind("HBP Handbrake"), Kind::Handbrake);
        assert_eq!(kind("HGP Shifter"), Kind::Other);
        assert_eq!(display_name("Gudsen MOZA R9 Base"), "MOZA R9 Base");
        assert_eq!(display_name("HBP Handbrake"), "MOZA HBP Handbrake");
        assert_eq!(model_id("Gudsen MOZA R9 Base"), "moza/r9-base");
    }
}

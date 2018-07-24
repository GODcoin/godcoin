#[repr(u8)]
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum AssetSymbol {
    GOLD,
    SILVER
}

impl AssetSymbol {
    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "GOLD" => Some(AssetSymbol::GOLD),
            "SILVER" => Some(AssetSymbol::SILVER),
            _ => None
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AssetSymbol::GOLD => "GOLD",
            AssetSymbol::SILVER => "SILVER"
        }
    }
}

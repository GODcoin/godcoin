#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum AssetSymbol {
    GOLD = 0u8,
    SILVER = 1u8,
}

impl AssetSymbol {
    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "GOLD" => Some(AssetSymbol::GOLD),
            "SILVER" => Some(AssetSymbol::SILVER),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AssetSymbol::GOLD => "GOLD",
            AssetSymbol::SILVER => "SILVER",
        }
    }
}

use std::cmp;
use crate::asset::*;

impl Asset {
    pub fn gt(&self, other: &Asset) -> Option<bool> {
        if self.symbol != other.symbol { return None }
        let decimals = cmp::max(self.decimals, other.decimals);
        let t = set_decimals_i64(self.amount, self.decimals, decimals)?;
        let o = set_decimals_i64(other.amount, other.decimals, decimals)?;
        Some(t > o)
    }

    pub fn geq(&self, other: &Asset) -> Option<bool> {
        if self.symbol != other.symbol { return None }
        let decimals = cmp::max(self.decimals, other.decimals);
        let t = set_decimals_i64(self.amount, self.decimals, decimals)?;
        let o = set_decimals_i64(other.amount, other.decimals, decimals)?;
        Some(t >= o)
    }

    pub fn lt(&self, other: &Asset) -> Option<bool> {
        Some(!(self.geq(other)?))
    }

    pub fn leq(&self, other: &Asset) -> Option<bool> {
        Some(!(self.gt(other)?))
    }

    pub fn eq(&self, other: &Asset) -> Option<bool> {
        if self.symbol != other.symbol { return None }
        let decimals = cmp::max(self.decimals, other.decimals);
        let t = set_decimals_i64(self.amount, self.decimals, decimals)?;
        let o = set_decimals_i64(other.amount, other.decimals, decimals)?;
        Some(t == o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_assets() {
        assert!(get_asset("1 GOLD").gt(&get_asset("0.50 GOLD")).unwrap());
        assert!(get_asset("1.0 GOLD").gt(&get_asset("0.99 GOLD")).unwrap());

        assert!(get_asset("1 GOLD").geq(&get_asset("1.0 GOLD")).unwrap());
        assert!(!get_asset("0.1 GOLD").geq(&get_asset("1.0 GOLD")).unwrap());

        assert!(get_asset("1 GOLD").leq(&get_asset("1.0 GOLD")).unwrap());
        assert!(get_asset("0.1 GOLD").leq(&get_asset("1.0 GOLD")).unwrap());
        assert!(get_asset("5.0 GOLD").leq(&get_asset("10 GOLD")).unwrap());

        assert!(get_asset("1 GOLD").eq(&get_asset("1 GOLD")).unwrap());
        assert!(!get_asset("1 GOLD").gt(&get_asset("1 GOLD")).unwrap());
        assert!(!get_asset("1 GOLD").lt(&get_asset("1 GOLD")).unwrap());
    }

    #[test]
    fn test_invalid_cmp() {
        let a = &get_asset("10 GOLD");
        let b = &get_asset("10 SILVER");

        assert!(a.gt(b).is_none());
        assert!(a.geq(b).is_none());
        assert!(a.lt(b).is_none());
        assert!(a.leq(b).is_none());
        assert!(a.eq(b).is_none());
    }

    fn get_asset(s: &str) -> Asset {
        Asset::from_str(s).unwrap()
    }
}

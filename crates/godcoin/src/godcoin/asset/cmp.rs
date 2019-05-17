use crate::asset::*;
use std::cmp;

impl Asset {
    pub fn gt(&self, other: &Asset) -> Option<bool> {
        let decimals = cmp::max(self.decimals, other.decimals);
        let t = set_decimals_i64(self.amount, self.decimals, decimals)?;
        let o = set_decimals_i64(other.amount, other.decimals, decimals)?;
        Some(t > o)
    }

    pub fn geq(&self, other: &Asset) -> Option<bool> {
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
        assert!(get_asset("1 GRAEL").gt(&get_asset("0.50 GRAEL")).unwrap());
        assert!(get_asset("1.0 GRAEL").gt(&get_asset("0.99 GRAEL")).unwrap());

        assert!(get_asset("1 GRAEL").geq(&get_asset("1.0 GRAEL")).unwrap());
        assert!(!get_asset("0.1 GRAEL").geq(&get_asset("1.0 GRAEL")).unwrap());

        assert!(get_asset("1 GRAEL").leq(&get_asset("1.0 GRAEL")).unwrap());
        assert!(get_asset("0.1 GRAEL").leq(&get_asset("1.0 GRAEL")).unwrap());
        assert!(get_asset("5.0 GRAEL").leq(&get_asset("10 GRAEL")).unwrap());

        assert!(get_asset("1 GRAEL").eq(&get_asset("1 GRAEL")).unwrap());
        assert!(get_asset("1.0 GRAEL").eq(&get_asset("1 GRAEL")).unwrap());
        assert!(get_asset("1.0 GRAEL").eq(&get_asset("1.0 GRAEL")).unwrap());
        assert!(get_asset("1 GRAEL").eq(&get_asset("1.00 GRAEL")).unwrap());
        assert!(get_asset("1 GRAEL").eq(&get_asset("1.0 GRAEL")).unwrap());
        assert!(get_asset("1.0 GRAEL").eq(&get_asset("1.00 GRAEL")).unwrap());

        assert!(!get_asset("1 GRAEL").gt(&get_asset("1 GRAEL")).unwrap());
        assert!(!get_asset("1.0 GRAEL").gt(&get_asset("1 GRAEL")).unwrap());
        assert!(!get_asset("1 GRAEL").gt(&get_asset("1.0 GRAEL")).unwrap());

        assert!(get_asset("-1 GRAEL").lt(&get_asset("1 GRAEL")).unwrap());
        assert!(!get_asset("1 GRAEL").lt(&get_asset("1 GRAEL")).unwrap());
        assert!(!get_asset("1 GRAEL").lt(&get_asset("1.0 GRAEL")).unwrap());
        assert!(!get_asset("1.0 GRAEL").lt(&get_asset("1 GRAEL")).unwrap());
        assert!(!get_asset("1 GRAEL").lt(&get_asset("-1 GRAEL")).unwrap());
    }

    fn get_asset(s: &str) -> Asset {
        Asset::from_str(s).unwrap()
    }
}

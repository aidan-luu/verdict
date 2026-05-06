use rust_decimal::Decimal;

pub fn brier_contribution(probability: Decimal, occurred: bool) -> Decimal {
    let outcome = if occurred {
        Decimal::ONE
    } else {
        Decimal::ZERO
    };
    let difference = probability - outcome;
    difference * difference
}

pub fn mean_brier(contributions: &[Decimal]) -> Decimal {
    if contributions.is_empty() {
        return Decimal::ZERO;
    }

    let total: Decimal = contributions.iter().copied().sum();
    total / Decimal::from(contributions.len() as u64)
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use crate::scoring::{brier_contribution, mean_brier};

    #[test]
    fn brier_contribution_matches_hand_computed_values() {
        assert_eq!(
            brier_contribution(Decimal::new(7, 1), true),
            Decimal::new(9, 2)
        );
        assert_eq!(
            brier_contribution(Decimal::new(2, 1), false),
            Decimal::new(4, 2)
        );
        assert_eq!(
            brier_contribution(Decimal::new(5, 1), true),
            Decimal::new(25, 2)
        );
    }

    #[test]
    fn mean_brier_matches_hand_computed_average() {
        let contributions = [Decimal::new(9, 2), Decimal::new(4, 2), Decimal::new(25, 2)];

        assert_eq!(
            mean_brier(&contributions),
            Decimal::from_str_exact("0.1266666666666666666666666667")
                .expect("decimal literal should parse")
        );
    }
}

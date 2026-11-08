//! The counters of a mask: `[C...]` in digits, `[Caa+1]` in letters.

use crate::pattern::{CounterSpec, CounterWidth, LetterCounter, MAX_COUNTER_WIDTH};
use crate::render::Fields;

/// The counter for the entry at `index`: start plus index times step (divided by the divisor of
/// `[C+1/100]`, the whole part), padded with zeros.
pub(crate) fn number(spec: CounterSpec, fields: &Fields<'_>) -> String {
    let settings = fields.counter;
    let start = i128::from(spec.start.unwrap_or(settings.start));
    let step = i128::from(spec.step.unwrap_or(settings.step));
    let divisor = i128::from(spec.divisor.unwrap_or(1).max(1));
    let value_at = |index: u64| start + (step * i128::from(index)).div_euclid(divisor);
    let width = match spec.width.unwrap_or(CounterWidth::Digits(settings.width)) {
        CounterWidth::Digits(width) => width.min(MAX_COUNTER_WIDTH),
        CounterWidth::Auto => {
            let last = value_at(fields.count.saturating_sub(1));
            let widest = start.unsigned_abs().max(last.unsigned_abs());
            widest.checked_ilog10().map_or(1, |digits| digits + 1)
        }
    };
    let width = usize::try_from(width).unwrap_or(1);
    let value = value_at(fields.index);
    let digits = format!("{:0width$}", value.unsigned_abs());
    if value < 0 {
        format!("-{digits}")
    } else {
        digits
    }
}

/// The letter counter for the entry at `index`: `a` is 0, `z` 25, at least `width` letters;
/// after the last value of a width (`zz`) the next width starts over (`aaa`). A value below 0
/// is the first one.
pub(crate) fn letters(spec: LetterCounter, fields: &Fields<'_>) -> String {
    let step = i128::from(spec.step.unwrap_or(fields.counter.step));
    let value = i128::from(spec.start) + step * i128::from(fields.index);
    let mut value = u128::try_from(value).unwrap_or_default();
    let mut width = spec.width.max(1);
    while let Some(values) = 26_u128.checked_pow(width).filter(|values| value >= *values) {
        value -= values;
        width += 1;
    }
    let base = if spec.upper { b'A' } else { b'a' };
    let mut out = vec![base; usize::try_from(width).unwrap_or(1)];
    for slot in out.iter_mut().rev() {
        *slot = base + u8::try_from(value % 26).unwrap_or_default();
        value /= 26;
    }
    out.into_iter().map(char::from).collect()
}

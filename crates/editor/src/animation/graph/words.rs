//! What a transition's wire label and the inspector's headers say, in words.

use axcenity_anim::ANY_STATE;
use axcenity_anim::files::{Condition, StateMachineFile, Test, TransitionDef};

/// A number as a person would type it: `0.1`, `2`, `-0.25`.
pub fn number_words(value: f64) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0" } else { text }.to_owned()
}

/// One condition: "Speed > 0.1", "IsGrounded", "not IsGrounded" or "Jump".
pub fn condition_words(condition: &Condition) -> String {
    let name = if condition.parameter.is_empty() {
        "?"
    } else {
        condition.parameter.as_str()
    };
    match condition.test {
        Test::IsTrue | Test::Triggered => name.to_owned(),
        Test::IsFalse => format!("not {name}"),
        Test::Greater => format!("{name} > {}", number_words(condition.value)),
        Test::Less => format!("{name} < {}", number_words(condition.value)),
    }
}

/// When its exit time lets it go: "when Walk finishes", or `None` for no
/// exit time or one of 0 (straight away).
fn exit_words(transition: &TransitionDef) -> Option<String> {
    let exit = transition.exit_time.filter(|exit| exit.is_finite())?;
    let from = if transition.from == ANY_STATE {
        "the clip"
    } else {
        transition.from.as_str()
    };
    if exit <= 1e-6 {
        return None;
    }
    Some(if (exit - 1.0).abs() < 1e-3 {
        format!("when {from} finishes")
    } else if exit < 1.0 {
        format!("when {from} is {:.0}% through", exit * 100.0)
    } else {
        format!("after {from} plays {}×", number_words(exit))
    })
}

/// What a wire's label says: when the transition happens, such as
/// "Speed > 0.1", "Jump", "when Walk finishes" or "always".
pub fn when_words(transition: &TransitionDef) -> String {
    let conditions: Vec<String> = transition.conditions.iter().map(condition_words).collect();
    let exit = exit_words(transition);
    match (conditions.is_empty(), exit) {
        (false, Some(exit)) => format!("{}, {exit}", conditions.join(" and ")),
        (false, None) => conditions.join(" and "),
        (true, Some(exit)) => exit,
        (true, None) if transition.exit_time.is_some() => "always".to_owned(),
        (true, None) => "never: say when".to_owned(),
    }
}

/// A transition with no condition and no exit time never happens.
pub fn never(transition: &TransitionDef) -> bool {
    transition.conditions.is_empty() && transition.exit_time.is_none()
}

/// The transition inspector's header: "Transition: when all of these are
/// true, blend from Walk to Run."
pub fn transition_header(transition: &TransitionDef) -> String {
    let when = match transition.conditions.len() {
        0 => "when its time comes",
        1 => "when this is true",
        _ => "when all of these are true",
    };
    format!(
        "Transition: {when}, blend from {} to {}.",
        transition.from, transition.to
    )
}

/// A state inspector's header: what it does while it's playing.
pub fn state_header(file: &StateMachineFile, name: &str) -> String {
    let start = if file
        .starting_state()
        .is_some_and(|state| state.name == name)
    {
        " Entry is wired to it, so the Animator starts here."
    } else {
        ""
    };
    format!(
        "State: plays its motion while it's active. Its wires say where it goes next and when.{start}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(conditions: Vec<Condition>, exit_time: Option<f64>) -> TransitionDef {
        let mut transition = TransitionDef::new("Walk", "Run");
        transition.conditions = conditions;
        transition.exit_time = exit_time;
        transition
    }

    fn test(parameter: &str, test: Test, value: f64) -> Condition {
        Condition {
            parameter: parameter.into(),
            test,
            value,
        }
    }

    #[test]
    fn wire_labels_say_when_in_words() {
        let speed = test("Speed", Test::Greater, 0.1);
        assert_eq!(when_words(&wire(vec![speed.clone()], None)), "Speed > 0.1");
        assert_eq!(
            when_words(&wire(vec![test("Jump", Test::Triggered, 0.0)], None)),
            "Jump"
        );
        assert_eq!(
            when_words(&wire(vec![test("IsGrounded", Test::IsFalse, 0.0)], None)),
            "not IsGrounded"
        );
        assert_eq!(
            when_words(&wire(
                vec![speed, test("Speed", Test::Less, 2.0)],
                Some(1.0)
            )),
            "Speed > 0.1 and Speed < 2, when Walk finishes"
        );
        assert_eq!(
            when_words(&wire(Vec::new(), Some(1.0))),
            "when Walk finishes"
        );
        assert_eq!(
            when_words(&wire(Vec::new(), Some(0.9))),
            "when Walk is 90% through"
        );
        assert_eq!(when_words(&wire(Vec::new(), Some(0.0))), "always");
        assert_eq!(when_words(&wire(Vec::new(), None)), "never: say when");
        assert_eq!(number_words(0.30000000000000004), "0.3");
        assert_eq!(number_words(-0.0), "0");
    }

    #[test]
    fn the_transition_header_says_what_it_does() {
        let two = wire(
            vec![
                test("Speed", Test::Greater, 3.0),
                test("IsGrounded", Test::IsTrue, 0.0),
            ],
            None,
        );
        assert_eq!(
            transition_header(&two),
            "Transition: when all of these are true, blend from Walk to Run."
        );
    }
}

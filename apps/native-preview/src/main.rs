const STATUS: &str = "realtime-manim native preview scaffold\nrenderer: unselected";

fn main() {
    println!("{STATUS}");
}

#[cfg(test)]
mod tests {
    use super::STATUS;

    #[test]
    fn renderer_choice_is_not_frozen() {
        assert!(STATUS.contains("renderer: unselected"));
    }
}

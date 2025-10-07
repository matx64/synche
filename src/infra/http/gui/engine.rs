use minijinja::Environment;

pub fn init() -> Environment<'static> {
    let mut engine = Environment::new();

    engine
        .add_template("index", include_str!("../../../../gui/index.html"))
        .unwrap();

    engine
}

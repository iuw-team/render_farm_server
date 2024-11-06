#[macro_export]
macro_rules! parse_env {
    ($var_name: expr, $ty: ty) => {
        std::env::var($var_name)
            .ok()
            .map(|raw_value| raw_value.parse::<$ty>())
            .and_then(|opt| opt.ok())
    };
}

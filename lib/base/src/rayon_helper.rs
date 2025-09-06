#[macro_export]
macro_rules! join_all {
    ($e1:expr, $e2:expr) => {
        rayon::join($e1, $e2)
    };
    ($e1:expr, $e2:expr, $e3:expr) => {{
        let ((a, b), c) = rayon::join(|| join_all!($e1, $e2), $e3);
        (a, b, c)
    }};
    ($e1:expr, $e2:expr, $e3:expr, $e4:expr) => {{
        let ((a, b), (c, d)) = rayon::join(|| join_all!($e1, $e2), || join_all!($e3, $e4));
        (a, b, c, d)
    }};
    ($e1:expr, $e2:expr, $e3:expr, $e4:expr, $e5:expr) => {{
        let ((a, b, c, d), e) = rayon::join(|| join_all!($e1, $e2, $e3, $e4), $e5);
        (a, b, c, d, e)
    }};
    ($e1:expr, $e2:expr, $e3:expr, $e4:expr, $e5:expr, $e6:expr) => {{
        let ((a, b, c, d, e), f) = rayon::join(|| join_all!($e1, $e2, $e3, $e4, $e5), $e6);
        (a, b, c, d, e, f)
    }};
    ($e1:expr, $e2:expr, $e3:expr, $e4:expr, $e5:expr, $e6:expr, $e7:expr) => {{
        let ((a, b, c, d, e, f), g) = rayon::join(|| join_all!($e1, $e2, $e3, $e4, $e5, $e6), $e7);
        (a, b, c, d, e, f, g)
    }};
    ($e1:expr, $e2:expr, $e3:expr, $e4:expr, $e5:expr, $e6:expr, $e7:expr, $e8:expr) => {{
        let ((a, b, c, d, e, f, g), h) =
            rayon::join(|| join_all!($e1, $e2, $e3, $e4, $e5, $e6, $e7), $e8);
        (a, b, c, d, e, f, g, h)
    }};
    ($e1:expr, $e2:expr, $e3:expr, $e4:expr, $e5:expr, $e6:expr, $e7:expr, $e8:expr, $e9:expr) => {{
        let ((a, b, c, d, e, f, g, h), i) =
            rayon::join(|| join_all!($e1, $e2, $e3, $e4, $e5, $e6, $e7, $e8), $e9);
        (a, b, c, d, e, f, g, h, i)
    }};
    ($e1:expr, $e2:expr, $e3:expr, $e4:expr, $e5:expr, $e6:expr, $e7:expr, $e8:expr, $e9:expr, $e10:expr) => {{
        let ((a, b, c, d, e, f, g, h, i), j) = rayon::join(
            || join_all!($e1, $e2, $e3, $e4, $e5, $e6, $e7, $e8, $e9),
            $e10,
        );
        (a, b, c, d, e, f, g, h, i, j)
    }};
    ($e1:expr, $e2:expr, $e3:expr, $e4:expr, $e5:expr, $e6:expr, $e7:expr, $e8:expr, $e9:expr, $e10:expr, $e11:expr) => {{
        let ((a, b, c, d, e, f, g, h, i, j), k) = rayon::join(
            || join_all!($e1, $e2, $e3, $e4, $e5, $e6, $e7, $e8, $e9, $e10),
            $e11,
        );
        (a, b, c, d, e, f, g, h, i, j, k)
    }};
    ($e1:expr, $e2:expr, $e3:expr, $e4:expr, $e5:expr, $e6:expr, $e7:expr, $e8:expr, $e9:expr, $e10:expr, $e11:expr, $e12:expr) => {{
        let ((a, b, c, d, e, f, g, h, i, j, k), l) = rayon::join(
            || join_all!($e1, $e2, $e3, $e4, $e5, $e6, $e7, $e8, $e9, $e10, $e11),
            $e12,
        );
        (a, b, c, d, e, f, g, h, i, j, k, l)
    }};
    ($e1:expr, $e2:expr, $e3:expr, $e4:expr, $e5:expr, $e6:expr, $e7:expr, $e8:expr, $e9:expr, $e10:expr, $e11:expr, $e12:expr, $e13:expr) => {{
        let ((a, b, c, d, e, f, g, h, i, j, k, l), m) = rayon::join(
            || {
                join_all!(
                    $e1, $e2, $e3, $e4, $e5, $e6, $e7, $e8, $e9, $e10, $e11, $e12
                )
            },
            $e13,
        );
        (a, b, c, d, e, f, g, h, i, j, k, l, m)
    }};
}

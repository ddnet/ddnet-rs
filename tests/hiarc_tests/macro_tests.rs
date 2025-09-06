#[test]
fn compile_test() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/hiarc_tests/compile-fail/*.rs");
}

#[test]
#[should_panic]
fn compile_test_unsafe_rc() {
    use hiarc::hiarc;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[hiarc]
    pub struct A {
        r: Rc<RefCell<String>>,
    }

    impl A {
        pub fn call_me(&self) {
            let mut r = self.r.borrow_mut();
            r.push_str("test A");
        }
    }

    #[hiarc]
    pub struct B {
        r: Rc<RefCell<String>>,
        a: A,
    }

    impl B {
        pub fn call_me(&self) {
            let mut r = self.r.borrow_mut();
            self.a.call_me();
            r.push_str("test B");
        }
    }

    let r = Rc::new(RefCell::new(String::new()));
    let b = B {
        r: r.clone(),
        a: A { r: r.clone() },
    };
    b.call_me();
    println!("{}", *r.borrow());
}

/// in contrast to `compile_test_unsafe_rc` this enforces safety of the borrow
#[test]
fn compile_test_safe_rc() {
    use hiarc::Hiarc;
    use hiarc::hiarc;
    use hiarc::hiarc_safer_rc_refcell;

    #[hiarc_safer_rc_refcell]
    #[derive(Debug, Hiarc, Default)]
    pub struct R {
        r: String,
    }

    #[hiarc_safer_rc_refcell]
    impl R {
        pub(crate) fn push_str(&mut self, s: &str) {
            self.r.push_str(s)
        }

        pub(crate) fn get_copy(&mut self) -> String {
            self.r.clone()
        }
    }

    #[hiarc]
    pub struct A {
        r: R,
    }

    impl A {
        pub fn call_me(&self) {
            self.r.push_str("test A");
        }
    }

    #[hiarc]
    pub struct B {
        r: R,
        a: A,
    }

    impl B {
        pub fn call_me(&self) {
            let r = &self.r;
            self.a.call_me();
            r.push_str("test B");
        }
    }

    let r = R::default();
    let b = B {
        r: r.clone(),
        a: A { r: r.clone() },
    };
    b.call_me();
    println!("{}", r.get_copy());
}

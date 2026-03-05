//! FormCalc conformance tests — based on XFA 3.3 §25 spec examples.
//!
//! Tests cover all language features and built-in functions as specified
//! in the FormCalc reference. Each test group corresponds to a section
//! of the specification.

use formcalc_interpreter::interpreter::Interpreter;
use formcalc_interpreter::lexer::tokenize;
use formcalc_interpreter::parser;
use formcalc_interpreter::value::Value;

fn run(src: &str) -> Value {
    let tokens = tokenize(src).expect(&format!("Tokenize failed for: {src}"));
    let ast = parser::parse(tokens).expect(&format!("Parse failed for: {src}"));
    let mut interp = Interpreter::new();
    interp.exec(&ast).expect(&format!("Exec failed for: {src}"))
}

fn run_f64(src: &str) -> f64 {
    match run(src) {
        Value::Number(n) => n,
        other => panic!("Expected Number, got {other:?} for: {src}"),
    }
}

fn run_str(src: &str) -> String {
    match run(src) {
        Value::String(s) => s,
        other => panic!("Expected String, got {other:?} for: {src}"),
    }
}

// ============================================================
// §25.4 — Arithmetic expressions
// ============================================================

#[test]
fn spec_arithmetic_basic() {
    assert_eq!(run_f64("1 + 2"), 3.0);
    assert_eq!(run_f64("3 - 1"), 2.0);
    assert_eq!(run_f64("2 * 3"), 6.0);
    assert_eq!(run_f64("10 / 4"), 2.5);
}

#[test]
fn spec_arithmetic_precedence() {
    assert_eq!(run_f64("2 + 3 * 4"), 14.0);
    assert_eq!(run_f64("(2 + 3) * 4"), 20.0);
    assert_eq!(run_f64("10 - 2 * 3"), 4.0);
    assert_eq!(run_f64("10 / 2 + 3"), 8.0);
}

#[test]
fn spec_arithmetic_unary_minus() {
    assert_eq!(run_f64("-5"), -5.0);
    assert_eq!(run_f64("-(3 + 4)"), -7.0);
    assert_eq!(run_f64("-(-5)"), 5.0);
}

#[test]
fn spec_arithmetic_mixed_types() {
    // Numbers coerced from strings
    assert_eq!(run_f64(r#""3" + 4"#), 7.0);
    assert_eq!(run_f64(r#""10" * "2""#), 20.0);
}

// ============================================================
// §25.5 — Comparison operators
// ============================================================

#[test]
fn spec_comparison_operators() {
    assert_eq!(run_f64("1 == 1"), 1.0);
    assert_eq!(run_f64("1 == 2"), 0.0);
    assert_eq!(run_f64("1 <> 2"), 1.0);
    assert_eq!(run_f64("1 <> 1"), 0.0);
    assert_eq!(run_f64("1 < 2"), 1.0);
    assert_eq!(run_f64("2 < 1"), 0.0);
    assert_eq!(run_f64("2 <= 2"), 1.0);
    assert_eq!(run_f64("3 <= 2"), 0.0);
    assert_eq!(run_f64("2 > 1"), 1.0);
    assert_eq!(run_f64("1 > 2"), 0.0);
    assert_eq!(run_f64("2 >= 2"), 1.0);
    assert_eq!(run_f64("1 >= 2"), 0.0);
}

// ============================================================
// §25.6 — Logical operators
// ============================================================

#[test]
fn spec_logical_operators() {
    assert_eq!(run_f64("1 and 1"), 1.0);
    assert_eq!(run_f64("1 and 0"), 0.0);
    assert_eq!(run_f64("0 and 0"), 0.0);
    assert_eq!(run_f64("1 or 0"), 1.0);
    assert_eq!(run_f64("0 or 0"), 0.0);
    assert_eq!(run_f64("not 0"), 1.0);
    assert_eq!(run_f64("not 1"), 0.0);
}

#[test]
fn spec_logical_complex() {
    assert_eq!(run_f64("(1 > 0) and (2 > 1)"), 1.0);
    assert_eq!(run_f64("(1 > 2) or (3 > 2)"), 1.0);
    assert_eq!(run_f64("not (1 > 2)"), 1.0);
}

// ============================================================
// §25.7 — String concatenation
// ============================================================

#[test]
fn spec_string_concat() {
    assert_eq!(run_str(r#""hello" & " " & "world""#), "hello world");
    assert_eq!(run_str(r#""abc" & "def""#), "abcdef");
}

#[test]
fn spec_string_concat_coercion() {
    // Numbers get coerced to strings when concatenated
    assert_eq!(run_str(r#""value: " & 42"#), "value: 42");
}

// ============================================================
// §25.8 — Variables
// ============================================================

#[test]
fn spec_var_declaration() {
    assert_eq!(run_f64("var x = 10\nx"), 10.0);
    assert!(matches!(run("var x\nx"), Value::Null));
}

#[test]
fn spec_var_assignment() {
    assert_eq!(run_f64("var x = 1\nx = x + 1\nx"), 2.0);
    assert_eq!(run_f64("var x = 5\nvar y = 10\nx = y\nx"), 10.0);
}

// ============================================================
// §25.9 — If/elseif/else
// ============================================================

#[test]
fn spec_if_simple() {
    assert_eq!(
        run_f64("if 1 then\n  42\nendif"),
        42.0
    );
}

#[test]
fn spec_if_else() {
    assert_eq!(
        run_f64("var x = 10\nif x > 5 then\n  1\nelse\n  0\nendif"),
        1.0
    );
    assert_eq!(
        run_f64("var x = 3\nif x > 5 then\n  1\nelse\n  0\nendif"),
        0.0
    );
}

#[test]
fn spec_if_elseif() {
    let script = r#"
        var grade = 85
        if grade >= 90 then
            4
        elseif grade >= 80 then
            3
        elseif grade >= 70 then
            2
        else
            1
        endif
    "#;
    assert_eq!(run_f64(script), 3.0);
}

#[test]
fn spec_if_nested() {
    let script = r#"
        var a = 5
        var b = 10
        if a > 0 then
            if b > 5 then
                a + b
            else
                a
            endif
        else
            0
        endif
    "#;
    assert_eq!(run_f64(script), 15.0);
}

// ============================================================
// §25.10 — While loop
// ============================================================

#[test]
fn spec_while_basic() {
    let script = "var i = 0\nvar sum = 0\nwhile i < 5 do\n  i = i + 1\n  sum = sum + i\nendwhile\nsum";
    assert_eq!(run_f64(script), 15.0);
}

#[test]
fn spec_while_break() {
    let script = r#"
        var i = 0
        while i < 100 do
            i = i + 1
            if i == 10 then
                break
            endif
        endwhile
        i
    "#;
    assert_eq!(run_f64(script), 10.0);
}

#[test]
fn spec_while_continue() {
    let script = r#"
        var i = 0
        var sum = 0
        while i < 10 do
            i = i + 1
            if i == 5 then
                continue
            endif
            sum = sum + i
        endwhile
        sum
    "#;
    // Sum of 1..10 minus 5 = 55 - 5 = 50
    assert_eq!(run_f64(script), 50.0);
}

// ============================================================
// §25.11 — For loop
// ============================================================

#[test]
fn spec_for_upto() {
    assert_eq!(
        run_f64("var sum = 0\nfor i = 1 upto 10 do\n  sum = sum + i\nendfor\nsum"),
        55.0
    );
}

#[test]
fn spec_for_downto() {
    let script = r#"
        var result = 1
        for i = 5 downto 1 do
            result = result * i
        endfor
        result
    "#;
    assert_eq!(run_f64(script), 120.0); // 5!
}

#[test]
fn spec_for_step() {
    let script = r#"
        var sum = 0
        for i = 0 upto 10 step 2 do
            sum = sum + i
        endfor
        sum
    "#;
    // 0 + 2 + 4 + 6 + 8 + 10 = 30
    assert_eq!(run_f64(script), 30.0);
}

#[test]
fn spec_for_nested() {
    let script = r#"
        var total = 0
        for i = 1 upto 3 do
            for j = 1 upto 3 do
                total = total + 1
            endfor
        endfor
        total
    "#;
    assert_eq!(run_f64(script), 9.0);
}

// ============================================================
// §25.12 — User-defined functions
// ============================================================

#[test]
fn spec_func_basic() {
    let script = r#"
        func square(x)
            x * x
        endfunc
        square(7)
    "#;
    assert_eq!(run_f64(script), 49.0);
}

#[test]
fn spec_func_multiple_params() {
    let script = r#"
        func area(width, height)
            width * height
        endfunc
        area(5, 3)
    "#;
    assert_eq!(run_f64(script), 15.0);
}

#[test]
fn spec_func_recursive() {
    let script = r#"
        func factorial(n)
            if n <= 1 then
                1
            else
                n * factorial(n - 1)
            endif
        endfunc
        factorial(6)
    "#;
    assert_eq!(run_f64(script), 720.0);
}

#[test]
fn spec_func_fibonacci() {
    let script = r#"
        func fib(n)
            if n <= 1 then
                n
            else
                fib(n - 1) + fib(n - 2)
            endif
        endfunc
        fib(10)
    "#;
    assert_eq!(run_f64(script), 55.0);
}

#[test]
fn spec_func_calling_func() {
    let script = r#"
        func double(x)
            x * 2
        endfunc
        func quadruple(x)
            double(double(x))
        endfunc
        quadruple(5)
    "#;
    assert_eq!(run_f64(script), 20.0);
}

// ============================================================
// §25.13 — Built-in: Arithmetic functions
// ============================================================

#[test]
fn spec_builtin_abs() {
    assert_eq!(run_f64("Abs(-5)"), 5.0);
    assert_eq!(run_f64("Abs(5)"), 5.0);
    assert_eq!(run_f64("Abs(0)"), 0.0);
}

#[test]
fn spec_builtin_avg() {
    assert_eq!(run_f64("Avg(1, 2, 3, 4, 5)"), 3.0);
    assert_eq!(run_f64("Avg(10, 20)"), 15.0);
}

#[test]
fn spec_builtin_ceil() {
    assert_eq!(run_f64("Ceil(1.2)"), 2.0);
    assert_eq!(run_f64("Ceil(-1.2)"), -1.0);
    assert_eq!(run_f64("Ceil(1.0)"), 1.0);
}

#[test]
fn spec_builtin_count() {
    assert_eq!(run_f64("Count(1, 2, 3)"), 3.0);
    assert_eq!(run_f64("Count(10)"), 1.0);
}

#[test]
fn spec_builtin_floor() {
    assert_eq!(run_f64("Floor(1.8)"), 1.0);
    assert_eq!(run_f64("Floor(-1.2)"), -2.0);
    assert_eq!(run_f64("Floor(2.0)"), 2.0);
}

#[test]
fn spec_builtin_max() {
    assert_eq!(run_f64("Max(1, 5, 3, 2, 4)"), 5.0);
    assert_eq!(run_f64("Max(-1, -5, -3)"), -1.0);
}

#[test]
fn spec_builtin_min() {
    assert_eq!(run_f64("Min(1, 5, 3, 2, 4)"), 1.0);
    assert_eq!(run_f64("Min(-1, -5, -3)"), -5.0);
}

#[test]
fn spec_builtin_mod() {
    assert_eq!(run_f64("Mod(10, 3)"), 1.0);
    assert_eq!(run_f64("Mod(7, 2)"), 1.0);
    assert_eq!(run_f64("Mod(10, 5)"), 0.0);
}

#[test]
fn spec_builtin_round() {
    assert_eq!(run_f64("Round(3.456, 2)"), 3.46);
    assert_eq!(run_f64("Round(3.456, 0)"), 3.0);
    assert_eq!(run_f64("Round(3.456, 1)"), 3.5);
}

#[test]
fn spec_builtin_sum() {
    assert_eq!(run_f64("Sum(1, 2, 3, 4, 5)"), 15.0);
    assert_eq!(run_f64("Sum(100)"), 100.0);
}

// ============================================================
// §25.14 — Built-in: String functions
// ============================================================

#[test]
fn spec_builtin_at() {
    assert_eq!(run_f64(r#"At("hello world", "world")"#), 7.0);
    assert_eq!(run_f64(r#"At("hello", "xyz")"#), 0.0);
    assert_eq!(run_f64(r#"At("abcabc", "bc")"#), 2.0);
}

#[test]
fn spec_builtin_concat() {
    assert_eq!(run_str(r#"Concat("a", "b", "c")"#), "abc");
    assert_eq!(run_str(r#"Concat("Hello", " ", "World")"#), "Hello World");
}

#[test]
fn spec_builtin_left() {
    assert_eq!(run_str(r#"Left("hello world", 5)"#), "hello");
    assert_eq!(run_str(r#"Left("abc", 10)"#), "abc");
}

#[test]
fn spec_builtin_len() {
    assert_eq!(run_f64(r#"Len("hello")"#), 5.0);
    assert_eq!(run_f64(r#"Len("")"#), 0.0);
    assert_eq!(run_f64(r#"Len("hello world")"#), 11.0);
}

#[test]
fn spec_builtin_lower() {
    assert_eq!(run_str(r#"Lower("HELLO")"#), "hello");
    assert_eq!(run_str(r#"Lower("Hello World")"#), "hello world");
}

#[test]
fn spec_builtin_ltrim() {
    assert_eq!(run_str(r#"Ltrim("   hello")"#), "hello");
    assert_eq!(run_str(r#"Ltrim("hello")"#), "hello");
}

#[test]
fn spec_builtin_replace() {
    assert_eq!(
        run_str(r#"Replace("hello world", "world", "there")"#),
        "hello there"
    );
}

#[test]
fn spec_builtin_right() {
    assert_eq!(run_str(r#"Right("hello world", 5)"#), "world");
    assert_eq!(run_str(r#"Right("abc", 10)"#), "abc");
}

#[test]
fn spec_builtin_rtrim() {
    assert_eq!(run_str(r#"Rtrim("hello   ")"#), "hello");
}

#[test]
fn spec_builtin_space() {
    assert_eq!(run_str("Space(5)"), "     ");
    assert_eq!(run_str("Space(0)"), "");
}

#[test]
fn spec_builtin_stuff() {
    // Stuff(s, start, deleteLen, insert) — 1-based position
    assert_eq!(
        run_str(r#"Stuff("hello world", 7, 5, "there")"#),
        "hello there"
    );
    assert_eq!(
        run_str(r#"Stuff("abcdef", 3, 2, "XY")"#),
        "abXYef"
    );
}

#[test]
fn spec_builtin_substr() {
    assert_eq!(run_str(r#"Substr("hello world", 7, 5)"#), "world");
    assert_eq!(run_str(r#"Substr("hello", 1, 3)"#), "hel");
}

#[test]
fn spec_builtin_upper() {
    assert_eq!(run_str(r#"Upper("hello")"#), "HELLO");
    assert_eq!(run_str(r#"Upper("Hello World")"#), "HELLO WORLD");
}

#[test]
fn spec_builtin_uuid() {
    let uuid = run_str("Uuid()");
    // UUID format: 8-4-4-4-12
    assert_eq!(uuid.len(), 36);
    assert_eq!(uuid.chars().filter(|&c| c == '-').count(), 4);
}

// ============================================================
// §25.15 — Built-in: Logical functions
// ============================================================

#[test]
fn spec_builtin_choose() {
    assert_eq!(run_f64("Choose(2, 10, 20, 30)"), 20.0);
    assert_eq!(run_f64("Choose(1, 10, 20, 30)"), 10.0);
    assert_eq!(run_f64("Choose(3, 10, 20, 30)"), 30.0);
}

#[test]
fn spec_builtin_if() {
    assert_eq!(run_f64("If(1, 42, 99)"), 42.0);
    assert_eq!(run_f64("If(0, 42, 99)"), 99.0);
    assert_eq!(run_str(r#"If(1 > 0, "yes", "no")"#), "yes");
}

#[test]
fn spec_builtin_oneof() {
    assert_eq!(run_f64("Oneof(3, 1, 2, 3, 4, 5)"), 1.0);
    assert_eq!(run_f64("Oneof(9, 1, 2, 3, 4, 5)"), 0.0);
}

#[test]
fn spec_builtin_within() {
    assert_eq!(run_f64("Within(5, 1, 10)"), 1.0);
    assert_eq!(run_f64("Within(15, 1, 10)"), 0.0);
    assert_eq!(run_f64("Within(1, 1, 10)"), 1.0);
    assert_eq!(run_f64("Within(10, 1, 10)"), 1.0);
}

// ============================================================
// §25.16 — Built-in: Misc functions
// ============================================================

#[test]
fn spec_builtin_hasvalue() {
    assert_eq!(run_f64("HasValue(42)"), 1.0);
    assert_eq!(run_f64(r#"HasValue("hello")"#), 1.0);
    assert_eq!(run_f64("HasValue(Null())"), 0.0);
    assert_eq!(run_f64(r#"HasValue("")"#), 0.0);
}

#[test]
fn spec_builtin_null_func() {
    // Null() with parens is a function call returning null
    assert!(matches!(run("Null()"), Value::Null));
}

#[test]
fn spec_builtin_null() {
    // Null keyword without parens
    assert!(matches!(run("null"), Value::Null));
}

// ============================================================
// §25.17 — Built-in: Date/Time functions
// ============================================================

#[test]
fn spec_builtin_date() {
    // Date() returns days since 1900-01-01
    let d = run_f64("Date()");
    assert!(d > 45000.0); // sanity check: we're past 2023
}

#[test]
fn spec_builtin_date2num() {
    // 2003-07-22 as per XFA spec example
    assert_eq!(run_f64(r#"Date2Num("2003-07-22", "YYYY-MM-DD")"#), 37823.0);
}

#[test]
fn spec_builtin_num2date() {
    // Round-trip
    let date_str = run_str(r#"Num2Date(37823, "YYYY-MM-DD")"#);
    assert_eq!(date_str, "2003-07-22");
}

#[test]
fn spec_builtin_date_roundtrip() {
    // Date2Num and Num2Date should be inverses
    assert_eq!(
        run_f64(r#"Date2Num(Num2Date(40000, "YYYY-MM-DD"), "YYYY-MM-DD")"#),
        40000.0
    );
}

#[test]
fn spec_builtin_time() {
    let t = run_f64("Time()");
    assert!(t >= 0.0);
    assert!(t < 86400000.0); // less than 24h in ms
}

#[test]
fn spec_builtin_time2num() {
    assert_eq!(run_f64(r#"Time2Num("13:30:00", "HH:MM:SS")"#), 48600000.0);
}

#[test]
fn spec_builtin_num2time() {
    let time_str = run_str(r#"Num2Time(48600000, "HH:MM:SS")"#);
    assert_eq!(time_str, "13:30:00");
}

// ============================================================
// §25.18 — Built-in: Financial functions
// ============================================================

#[test]
fn spec_builtin_pmt() {
    // Monthly payment on $10000 loan at 8%/yr for 36 months
    let pmt = run_f64("Pmt(10000, 0.08 / 12, 36)");
    assert!((pmt - 313.36).abs() < 0.01);
}

#[test]
fn spec_builtin_fv() {
    // Future value: $100/mo, 6%/yr, 12 months
    let fv = run_f64("FV(100, 0.06 / 12, 12)");
    assert!((fv - 1233.56).abs() < 0.01);
}

#[test]
fn spec_builtin_pv() {
    // Present value: $100/mo, 6%/yr, 12 months
    let pv = run_f64("PV(100, 0.06 / 12, 12)");
    assert!((pv - 1161.89).abs() < 0.1);
}

#[test]
fn spec_builtin_rate() {
    let rate = run_f64("Rate(48000, 2000, 24)");
    assert!(rate > 0.0);
}

#[test]
fn spec_builtin_term() {
    let term = run_f64("Term(100, 0.05 / 12, 5000)");
    assert!(term > 0.0);
}

#[test]
fn spec_builtin_cterm() {
    let cterm = run_f64("CTerm(0.05 / 12, 10000, 5000)");
    assert!(cterm > 0.0);
}

#[test]
fn spec_builtin_npv() {
    let npv = run_f64("NPV(0.10, 100, 200, 300)");
    assert!(npv > 0.0);
}

// ============================================================
// §25.19 — Type coercion
// ============================================================

#[test]
fn spec_coercion_string_to_number() {
    assert_eq!(run_f64(r#""42" + 0"#), 42.0);
    assert_eq!(run_f64(r#""3.14" + 0"#), 3.14);
    assert_eq!(run_f64(r#""" + 0"#), 0.0); // empty string -> 0
}

#[test]
fn spec_coercion_number_to_string() {
    assert_eq!(run_str(r#"Concat(42, "")"#), "42");
}

#[test]
fn spec_coercion_null_behavior() {
    assert_eq!(run_f64("Null() + 5"), 5.0); // null coerces to 0
    assert_eq!(run_str(r#"Null() & "hello""#), "hello"); // null coerces to ""
}

// ============================================================
// §25.20 — Complex scripts
// ============================================================

#[test]
fn spec_complex_tax_calculation() {
    let script = r#"
        var subtotal = 250.00
        var tax_rate = 0.21
        var tax = Round(subtotal * tax_rate, 2)
        var total = subtotal + tax
        total
    "#;
    assert_eq!(run_f64(script), 302.50);
}

#[test]
fn spec_complex_fibonacci_loop() {
    let script = r#"
        var n = 10
        var a = 0
        var b = 1
        for i = 2 upto n do
            var temp = b
            b = a + b
            a = temp
        endfor
        b
    "#;
    assert_eq!(run_f64(script), 55.0);
}

#[test]
fn spec_complex_string_builder() {
    let script = r#"
        var result = ""
        for i = 1 upto 5 do
            if Len(result) > 0 then
                result = result & ", "
            endif
            result = result & i
        endfor
        result
    "#;
    assert_eq!(run_str(script), "1, 2, 3, 4, 5");
}

#[test]
fn spec_complex_nested_functions() {
    let script = r#"
        func gcd(a, b)
            if b == 0 then
                a
            else
                gcd(b, Mod(a, b))
            endif
        endfunc
        gcd(48, 18)
    "#;
    assert_eq!(run_f64(script), 6.0);
}

#[test]
fn spec_complex_accumulator() {
    let script = r#"
        func accumulate(n)
            var total = 0
            for i = 1 upto n do
                total = total + i * i
            endfor
            total
        endfunc
        accumulate(5)
    "#;
    // 1 + 4 + 9 + 16 + 25 = 55
    assert_eq!(run_f64(script), 55.0);
}

#[test]
fn spec_complex_discount_tiers() {
    let script = r#"
        func calc_discount(amount)
            if amount >= 1000 then
                Round(amount * 0.15, 2)
            elseif amount >= 500 then
                Round(amount * 0.10, 2)
            elseif amount >= 100 then
                Round(amount * 0.05, 2)
            else
                0
            endif
        endfunc

        var d1 = calc_discount(1500)
        var d2 = calc_discount(750)
        var d3 = calc_discount(200)
        var d4 = calc_discount(50)
        d1 + d2 + d3 + d4
    "#;
    // 225 + 75 + 10 + 0 = 310
    assert_eq!(run_f64(script), 310.0);
}

#[test]
fn spec_complex_prime_check() {
    let script = r#"
        func is_prime(n)
            if n < 2 then
                0
            else
                var result = 1
                var i = 2
                while i * i <= n do
                    if Mod(n, i) == 0 then
                        result = 0
                        break
                    endif
                    i = i + 1
                endwhile
                result
            endif
        endfunc

        var count = 0
        for i = 2 upto 20 do
            if is_prime(i) then
                count = count + 1
            endif
        endfor
        count
    "#;
    // Primes up to 20: 2,3,5,7,11,13,17,19 = 8
    assert_eq!(run_f64(script), 8.0);
}

#[test]
fn spec_complex_wordnum() {
    let w = run_str("WordNum(1234, 1)");
    assert!(w.contains("One Thousand"));
    assert!(w.contains("Two Hundred"));
    assert!(w.contains("Thirty"));
    assert!(w.contains("Four"));
}

// ============================================================
// §25.21 — Edge cases
// ============================================================

#[test]
fn edge_division_by_zero() {
    let tokens = tokenize("1 / 0").unwrap();
    let ast = parser::parse(tokens).unwrap();
    let mut interp = Interpreter::new();
    assert!(interp.exec(&ast).is_err());
}

#[test]
fn edge_empty_script() {
    // Empty body should return Null
    assert!(matches!(run(""), Value::Null));
}

#[test]
fn edge_multiple_expressions() {
    // Last expression value is returned
    assert_eq!(run_f64("1\n2\n3"), 3.0);
}

#[test]
fn edge_nested_loops() {
    let script = r#"
        var sum = 0
        for i = 1 upto 5 do
            for j = 1 upto 5 do
                sum = sum + 1
            endfor
        endfor
        sum
    "#;
    assert_eq!(run_f64(script), 25.0);
}

#[test]
fn edge_function_scope() {
    // Variables inside functions shouldn't leak
    let script = r#"
        var x = 10
        func set_x()
            var x = 99
            x
        endfunc
        set_x()
        x
    "#;
    assert_eq!(run_f64(script), 10.0);
}

#[test]
fn edge_case_insensitive_builtins() {
    // FormCalc built-ins are case-insensitive
    assert_eq!(run_f64("abs(-5)"), 5.0);
    assert_eq!(run_f64("ABS(-5)"), 5.0);
    assert_eq!(run_f64("Abs(-5)"), 5.0);
    assert_eq!(run_str(r#"upper("hello")"#), "HELLO");
    assert_eq!(run_str(r#"UPPER("hello")"#), "HELLO");
}

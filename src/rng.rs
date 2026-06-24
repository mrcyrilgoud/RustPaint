/// Inline xorshift32 PRNG — no external crate needed.
pub fn next_rand(state: &mut u32) -> u32 {
    if *state == 0 {
        *state = 0xdeadbeef;
    }
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

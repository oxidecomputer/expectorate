// Copyright 2020 Oxide Computer Company

use expectorate::assert_contents;

fn main() {
    let actual = compose();
    assert_contents("examples/lyrics.txt", actual);
}

fn compose() -> &'static str {
    "In a testing match nobody tests like Gaston\nI'm especially good at \
     expectorating\n"
}

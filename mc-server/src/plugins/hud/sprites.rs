//! HUD 그림 자료: 5×7 글자(숫자·몇몇 기호)와 9×9 아이콘. 그리는 법은 [`super::art`].

pub const COIN: char = '\u{E100}';
pub const GEM: char = '\u{E101}';
pub const MOON: char = '\u{E102}';
pub const STAR: char = '\u{E103}';
pub const SWORD: char = '\u{E104}';
pub const SHIELD: char = '\u{E105}';
pub const HEART: char = '\u{E106}';
pub const BOOT: char = '\u{E107}';
pub const FLAME: char = '\u{E108}';
pub const DROP: char = '\u{E109}';
pub const SCROLL: char = '\u{E10A}';

/// 5×7 글자 (`#` 채움). 가장자리 1px은 코드가 어둡게 둘러 줍니다.
pub const LETTERS: &[(char, [&str; 7])] = &[
    (
        '0',
        [
            ".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###.",
        ],
    ),
    (
        '1',
        [
            "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.",
        ],
    ),
    (
        '2',
        [
            ".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####",
        ],
    ),
    (
        '3',
        [
            ".###.", "#...#", "....#", "..##.", "....#", "#...#", ".###.",
        ],
    ),
    (
        '4',
        [
            "...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#.",
        ],
    ),
    (
        '5',
        [
            "#####", "#....", "####.", "....#", "....#", "#...#", ".###.",
        ],
    ),
    (
        '6',
        [
            "..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###.",
        ],
    ),
    (
        '7',
        [
            "#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...",
        ],
    ),
    (
        '8',
        [
            ".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###.",
        ],
    ),
    (
        '9',
        [
            ".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##..",
        ],
    ),
    (',', ["..", "..", "..", "..", ".#", ".#", "#."]),
    ('.', [".", ".", ".", ".", ".", ".", "#"]),
    (
        '%',
        [
            "##..#", "##..#", "...#.", "..#..", ".#...", "#..##", "#..##",
        ],
    ),
    (
        '/',
        [
            "....#", "...#.", "...#.", "..#..", ".#...", ".#...", "#....",
        ],
    ),
    (
        '+',
        [
            ".....", "..#..", "..#..", "#####", "..#..", "..#..", ".....",
        ],
    ),
    (
        '-',
        [
            ".....", ".....", ".....", "#####", ".....", ".....", ".....",
        ],
    ),
    (
        'K',
        [
            "#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#",
        ],
    ),
    (
        'M',
        [
            "#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#",
        ],
    ),
    (
        'B',
        [
            "####.", "#...#", "#...#", "####.", "#...#", "#...#", "####.",
        ],
    ),
    (
        'L',
        [
            "#....", "#....", "#....", "#....", "#....", "#....", "#####",
        ],
    ),
    (
        'v',
        [
            ".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#..",
        ],
    ),
];

/// 9×9 아이콘과 팔레트.
pub type Icon = (char, [&'static str; 9], &'static [(char, u32)]);

pub const ICONS: &[Icon] = &[
    (
        COIN,
        [
            "...ddd...",
            ".ddyyydd.",
            ".dyYYyyd.",
            "dyYyyyyod",
            "dyYyyyyod",
            "dyyyyyyod",
            ".dyyyood.",
            ".ddooodd.",
            "...ddd...",
        ],
        &[
            ('d', 0x7a4a00ff),
            ('y', 0xf5c542ff),
            ('Y', 0xfff3b0ff),
            ('o', 0xc9901aff),
        ],
    ),
    (
        GEM,
        [
            ".........",
            "..ddddd..",
            ".dCCcbbd.",
            "dCCccbbbd",
            ".dcccbbd.",
            "..dccbd..",
            "...dcd...",
            "....d....",
            ".........",
        ],
        &[
            ('d', 0x123a6bff),
            ('c', 0x55e0ffff),
            ('C', 0xc8f7ffff),
            ('b', 0x2a8fd6ff),
        ],
    ),
    (
        MOON,
        [
            "...ddd...",
            "..dmmmd..",
            ".dmmdd...",
            ".dmd...z.",
            ".dmd..zzz",
            ".dmmdd.z.",
            "..dmmmd..",
            "...ddd...",
            ".........",
        ],
        &[('d', 0x6b5a1eff), ('m', 0xfff2a8ff), ('z', 0xbfe3ffff)],
    ),
    (
        STAR,
        [
            "....d....",
            "...dyd...",
            "dddyyyddd",
            "dyyyYyyyd",
            ".dyyyyyd.",
            "..dyyyd..",
            ".dyydyyd.",
            "dyd...dyd",
            "dd.....dd",
        ],
        &[('d', 0x7a5200ff), ('y', 0xffd84aff), ('Y', 0xfff6c0ff)],
    ),
    (
        SWORD,
        [
            "......ddd",
            ".....dssd",
            "....dssd.",
            "d..dssd..",
            ".ddssd...",
            "..dhd....",
            ".dhdd....",
            "dhd.d....",
            "dd.......",
        ],
        &[('d', 0x2e2e2eff), ('s', 0xe6e6e6ff), ('h', 0x8b5a2bff)],
    ),
    (
        SHIELD,
        [
            "ddddddddd",
            "dsssbsssd",
            "dsssbsssd",
            "dbbbbbbbd",
            "dsssbsssd",
            ".dssbssd.",
            "..dsbsd..",
            "...dsd...",
            "....d....",
        ],
        &[('d', 0x22324aff), ('s', 0x6fa0d8ff), ('b', 0xf2d16bff)],
    ),
    (
        HEART,
        [
            ".dd...dd.",
            "dRrd.dRrd",
            "dRrrdrrrd",
            "drrrrrrrd",
            "drrrrrrrd",
            ".drrrrrd.",
            "..drrrd..",
            "...drd...",
            "....d....",
        ],
        &[('d', 0x5a0a10ff), ('r', 0xe8323cff), ('R', 0xff9a9aff)],
    ),
    (
        BOOT,
        [
            "..ddddd..",
            "..dbbbd..",
            "..dbbbd..",
            "..dbbbd..",
            "..dbbbdd.",
            ".dbbbbbbd",
            "dbbbbbbbd",
            "ddddddddd",
            ".........",
        ],
        &[('d', 0x3e2412ff), ('b', 0x9b6a3cff)],
    ),
    (
        FLAME,
        [
            "....d....",
            "...dod...",
            "..doood..",
            "..dooood.",
            ".doyyood.",
            ".doyyyod.",
            "doyyyyyod",
            ".doyyyod.",
            "..ddddd..",
        ],
        &[('d', 0x7a1e00ff), ('o', 0xff7a1aff), ('y', 0xffe066ff)],
    ),
    (
        DROP,
        [
            "....d....",
            "...dbd...",
            "..dbbbd..",
            ".dbBbbbd.",
            ".dbBbbbd.",
            "dbBbbbbbd",
            "dbbbbbbnd",
            ".dbbbbnd.",
            "..ddddd..",
        ],
        &[
            ('d', 0x0f2a5aff),
            ('b', 0x3a8cffff),
            ('B', 0xb8dcffff),
            ('n', 0x2060c0ff),
        ],
    ),
    (
        SCROLL,
        [
            ".ddddddd.",
            "dpPPPPPpd",
            ".dpllllpd",
            ".dpPPPPpd",
            ".dpllllpd",
            ".dpPPPPpd",
            ".dpllPPpd",
            "dpPPPPPpd",
            ".ddddddd.",
        ],
        &[
            ('d', 0x4a3010ff),
            ('p', 0xd9b77aff),
            ('P', 0xf6e3b4ff),
            ('l', 0x8a6a3aff),
        ],
    ),
];

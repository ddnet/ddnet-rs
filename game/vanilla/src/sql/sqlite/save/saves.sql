CREATE TABLE user_save (
    id INTEGER AUTO_INCREMENT,
    user_id INTEGER UNIQUE,
    user_hash BINARY(32) UNIQUE,
    score_laser_kills INTEGER NOT NULL DEFAULT 0,
    score_deaths INTEGER NOT NULL DEFAULT 0,
    score_hits INTEGER NOT NULL DEFAULT 0,
    score_teamkills INTEGER NOT NULL DEFAULT 0,
    score_suicides INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(id)
);

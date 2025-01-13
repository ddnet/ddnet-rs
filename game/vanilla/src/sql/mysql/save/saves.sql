CREATE TABLE user_save (
    id BIGINT NOT NULL AUTO_INCREMENT,
    user_id BIGINT,
    user_hash BINARY(32),
    score_laser_kills BIGINT NOT NULL DEFAULT 0,
    score_deaths BIGINT NOT NULL DEFAULT 0,
    score_hits BIGINT NOT NULL DEFAULT 0,
    score_teamkills BIGINT NOT NULL DEFAULT 0,
    score_suicides BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY(id),
    UNIQUE KEY(user_id),
    UNIQUE KEY(user_hash)
);

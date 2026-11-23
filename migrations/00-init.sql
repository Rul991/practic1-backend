CREATE TYPE user_role AS ENUM ('user', 'admin');
CREATE TYPE question_type AS ENUM ('single_choice', 'multiple_choice', 'text', 'number', 'boolean');

CREATE TABLE users (
    id            BIGSERIAL    PRIMARY KEY,
    login         VARCHAR(64)  NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    name          VARCHAR(128) NOT NULL,
    role          user_role    NOT NULL DEFAULT 'user',
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE TABLE surveys (
    id                BIGSERIAL    PRIMARY KEY,
    owner_id          BIGINT       NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title             VARCHAR(255) NOT NULL,
    description       TEXT,
    is_anonymous      BOOLEAN      NOT NULL DEFAULT FALSE,
    only_one_response BOOLEAN      NOT NULL DEFAULT FALSE,
    link              VARCHAR(64)  NOT NULL UNIQUE,
    created_at        TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ  NOT NULL DEFAULT now(),
    CONSTRAINT chk_link_slug CHECK (link ~ '^[A-Za-z0-9_-]+$')
);
CREATE INDEX idx_surveys_owner ON surveys(owner_id);

CREATE TABLE survey_questions (
    id          BIGSERIAL     PRIMARY KEY,
    survey_id   BIGINT        NOT NULL REFERENCES surveys(id) ON DELETE CASCADE,
    text        TEXT          NOT NULL,
    type        question_type NOT NULL,
    is_required BOOLEAN       NOT NULL DEFAULT FALSE,
    position    INT           NOT NULL,
    CONSTRAINT uq_question_position UNIQUE (survey_id, position)
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT uq_question_id_survey UNIQUE (id, survey_id)
);
CREATE INDEX idx_questions_survey ON survey_questions(survey_id, position);

CREATE TABLE question_options (
    id          BIGSERIAL PRIMARY KEY,
    question_id BIGINT    NOT NULL REFERENCES survey_questions(id) ON DELETE CASCADE,
    text        TEXT      NOT NULL,
    position    INT       NOT NULL,
    CONSTRAINT uq_option_position UNIQUE (question_id, position)
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT uq_option_id_question UNIQUE (id, question_id)
);
CREATE INDEX idx_options_question ON question_options(question_id, position);

CREATE TABLE responses (
    id         BIGSERIAL   PRIMARY KEY,
    survey_id  BIGINT      NOT NULL REFERENCES surveys(id) ON DELETE CASCADE,
    user_id    BIGINT      NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_responses_survey      ON responses(survey_id);
CREATE INDEX idx_responses_user        ON responses(user_id);
CREATE INDEX idx_responses_survey_user ON responses(survey_id, user_id);

CREATE TABLE answers (
    id            BIGSERIAL PRIMARY KEY,
    response_id   BIGINT    NOT NULL REFERENCES responses(id)        ON DELETE CASCADE,
    question_id   BIGINT    NOT NULL REFERENCES survey_questions(id) ON DELETE CASCADE,
    option_id     BIGINT,
    text_value    VARCHAR(4096),
    numeric_value NUMERIC,
    bool_value    BOOLEAN,
    CONSTRAINT fk_option_belongs_to_question
        FOREIGN KEY (option_id, question_id)
        REFERENCES question_options(id, question_id) ON DELETE CASCADE,
    CONSTRAINT chk_one_value CHECK (
        (option_id     IS NOT NULL)::int +
        (text_value    IS NOT NULL)::int +
        (numeric_value IS NOT NULL)::int +
        (bool_value    IS NOT NULL)::int = 1
    ),
    CONSTRAINT uq_answer UNIQUE NULLS NOT DISTINCT (response_id, question_id, option_id)
);
CREATE INDEX idx_answers_response ON answers(response_id);
CREATE INDEX idx_answers_question ON answers(question_id);
CREATE INDEX idx_answers_option   ON answers(option_id) WHERE option_id IS NOT NULL;
CREATE INDEX idx_answers_agg      ON answers(question_id, option_id) WHERE option_id IS NOT NULL;
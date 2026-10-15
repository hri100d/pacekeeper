use ::redis::{Cmd, Script, cmd};

use crate::{BucketConfig, BucketCounter};

const ACQUIRE_SCRIPT: &str = include_str!("acquire.lua");

#[derive(Clone, Copy)]
enum RedisCommand {
    Eval,
    EvalSha,
    Delete,
}

impl RedisCommand {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Eval => "EVAL",
            Self::EvalSha => "EVALSHA",
            Self::Delete => "DEL",
        }
    }
}

pub(super) struct RedisCommandFactory<C: BucketCounter, R: BucketCounter = u64> {
    config: BucketConfig<C, R>,
    script: Script,
}

impl<C: BucketCounter, R: BucketCounter> RedisCommandFactory<C, R> {
    pub(super) fn new(config: BucketConfig<C, R>) -> Self {
        Self {
            config,
            script: Script::new(ACQUIRE_SCRIPT),
        }
    }

    pub(super) fn config(&self) -> BucketConfig<C, R> {
        self.config
    }

    pub(super) fn evalsha(&self, key: &str) -> Cmd {
        self.make_command(RedisCommand::EvalSha, key)
    }

    pub(super) fn eval(&self, key: &str) -> Cmd {
        self.make_command(RedisCommand::Eval, key)
    }

    pub(super) fn delete(&self, key: &str) -> Cmd {
        self.make_command(RedisCommand::Delete, key)
    }

    fn make_command(&self, kind: RedisCommand, key: &str) -> Cmd {
        let mut command = cmd(kind.as_str());

        match kind {
            RedisCommand::Eval | RedisCommand::EvalSha => {
                match kind {
                    RedisCommand::Eval => {
                        command.arg(ACQUIRE_SCRIPT);
                    }
                    RedisCommand::EvalSha => {
                        command.arg(self.script.get_hash());
                    }
                    RedisCommand::Delete => unreachable!(),
                }

                command
                    .arg(1)
                    .arg(key)
                    .arg(self.config.max_credit())
                    .arg(self.config.period_us())
                    .arg(self.config.refill_tokens_u64());
            }

            RedisCommand::Delete => {
                command.arg(key);
            }
        }

        command
    }
}

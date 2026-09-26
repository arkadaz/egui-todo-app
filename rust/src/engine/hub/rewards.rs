//! Rewards: things to look forward to, unfinished ones first.

use super::*;

impl Hub {
    /// Rewards, unfinished ones first (like the desktop app).
    pub fn rewards(&self) -> &[Reward] {
        &self.data.rewards
    }

    pub fn add_reward(&mut self, name: &str, now_ms: i64) -> Result<()> {
        let name = checked_text(name, "a reward")?;
        self.reload_if_changed()?;
        self.data.rewards.push(Reward { name, completed: false });
        sort_rewards(&mut self.data.rewards);
        self.save(now_ms, [Change::Rewards])
    }

    pub fn set_reward_completed(&mut self, index: usize, completed: bool, now_ms: i64) -> Result<()> {
        self.reload_if_changed()?;
        let reward = self
            .data
            .rewards
            .get_mut(index)
            .context("That reward no longer exists.")?;
        reward.completed = completed;
        sort_rewards(&mut self.data.rewards);
        self.save(now_ms, [Change::Rewards])
    }

    pub fn edit_reward(&mut self, index: usize, name: &str, now_ms: i64) -> Result<()> {
        let name = checked_text(name, "a reward")?;
        self.reload_if_changed()?;
        self.data
            .rewards
            .get_mut(index)
            .context("That reward no longer exists.")?
            .name = name;
        self.save(now_ms, [Change::Rewards])
    }

    /// Deletes a reward and returns it (so it can be restored with `restore_reward`).
    pub fn delete_reward(&mut self, index: usize, now_ms: i64) -> Result<Reward> {
        self.reload_if_changed()?;
        if index >= self.data.rewards.len() {
            bail!("That reward no longer exists.");
        }
        let removed = self.data.rewards.remove(index);
        self.save(now_ms, [Change::Rewards])?;
        Ok(removed)
    }

    /// Puts a deleted reward back (for "Undo").
    pub fn restore_reward(&mut self, index: usize, reward: Reward, now_ms: i64) -> Result<()> {
        self.reload_if_changed()?;
        let index = index.min(self.data.rewards.len());
        self.data.rewards.insert(index, reward);
        sort_rewards(&mut self.data.rewards);
        self.save(now_ms, [Change::Rewards])
    }
}

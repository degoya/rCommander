//! `Engine::dispatch` for the `queue` group (`registry/queue.rs`).

/// The `queue` group's arms, in the order of its rows (`dispatch/arms.rs`).
macro_rules! arms {
    ($next:ident, $state:tt, $self:ident, { $($done:tt)* }) => {
        $next! { $state, $self, { $($done)*
            Command::QueueAssign { job, list } => $self.queue_assign(job, list),
            Command::QueueAssignMany { jobs, list } => $self.queue_assign_many(&jobs, list),
            Command::QueueClear { list, all_lists } => $self.queue_clear(list, all_lists),
            Command::QueueClearFinished {} => Ok($self.queue_clear_finished()),
            Command::QueueCreate { name, jobs } => $self.queue_create(&name, &jobs).await,
            Command::QueueDelete { list } => $self.queue_delete(list).await,
            Command::QueueDeleteSaved { saved } => $self.queue_delete_saved(saved).await,
            Command::QueueDiscard { jobs } => $self.queue_discard(&jobs),
            Command::QueueList {} => Ok($self.queue_list()),
            Command::QueueLists {} => $self.queue_lists().await,
            Command::QueueLoad { saved } => $self.queue_load(saved).await,
            Command::QueueMove { job, step } => $self.queue_move(job, step),
            Command::QueueMoveMany { jobs, step } => $self.queue_move_many(&jobs, step),
            Command::QueuePause { list } => $self.queue_pause(list).await,
            Command::QueuePauseAll {} => Ok($self.queue_pause_all()),
            Command::QueueRemove { job } => $self.queue_remove(job),
            Command::QueueRename { list, name } => $self.queue_rename(list, &name).await,
            Command::QueueResumeAll {} => Ok($self.queue_resume_all()),
            Command::QueueSave {
                list,
                name,
                restore_on_start,
            } => $self.queue_save(list, name, restore_on_start).await,
            Command::QueueStart { list } => $self.queue_start(list).await,
            Command::QueueWindow {} => Ok($self.queue_window()),
        } }
    };
}
pub(crate) use arms;

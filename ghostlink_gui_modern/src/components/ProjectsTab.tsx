import React, { useState, useEffect, useCallback } from 'react';
import { Plus, FolderGit2, Loader2 } from 'lucide-react';
import { Project, Task, GhostlinkAPI } from '../api';
import { useAppStore } from '../store';
import { TaskView } from './TaskView';

interface ProjectsTabProps {
  api?: GhostlinkAPI;
}

export const ProjectsTab: React.FC<ProjectsTabProps> = ({ api }) => {
  const activeApi = api!;

  const projects = useAppStore((state) => state.projects);
  const setProjects = useAppStore((state) => state.setProjects);
  const activeProject = useAppStore((state) => state.activeProject);
  const setActiveProject = useAppStore((state) => state.setActiveProject);
  const tasks = useAppStore((state) => state.tasks);
  const setTasks = useAppStore((state) => state.setTasks);
  const activeTask = useAppStore((state) => state.activeTask);
  const setActiveTask = useAppStore((state) => state.setActiveTask);
  const addToast = useAppStore((state) => state.addToast);
  const setUnreadNeedsReviewCount = useAppStore((state) => state.setUnreadNeedsReviewCount);

  const [showNewProjModal, setShowNewProjModal] = useState<boolean>(false);
  const [isCreatingProj, setIsCreatingProj] = useState<boolean>(false);
  const [projName, setProjName] = useState<string>('');
  const [projRootPath, setProjRootPath] = useState<string>('');
  const [projKind] = useState<'code' | 'work'>('code');

  const [showNewTaskModal, setShowNewTaskModal] = useState<boolean>(false);
  const [isCreatingTask, setIsCreatingTask] = useState<boolean>(false);
  const [taskGoal, setTaskGoal] = useState<string>('');
  const [taskAcceptance, setTaskAcceptance] = useState<string>('');

  const fetchProjects = useCallback(async () => {
    try {
      const list = await activeApi.listProjects();
      setProjects(list);
      if (list.length > 0 && !activeProject) {
        setActiveProject(list[0]);
      }
    } catch (err: any) {
      addToast({ type: 'error', message: err.message || 'Failed to fetch projects' });
    }
  }, [activeApi, setProjects, activeProject, setActiveProject, addToast]);

  const fetchTasks = useCallback(async () => {
    if (!activeProject) return;
    try {
      const list = await activeApi.listProjectTasks(activeProject.id);
      setTasks(list);
      const needsReview = list.filter((t) => t.status === 'needs_review').length;
      setUnreadNeedsReviewCount(needsReview);
    } catch {
      /* ignore */
    }
  }, [activeApi, activeProject, setTasks, setUnreadNeedsReviewCount]);

  useEffect(() => {
    fetchProjects();
  }, [fetchProjects]);

  useEffect(() => {
    if (activeProject) {
      fetchTasks();
    }
  }, [activeProject, fetchTasks]);

  useEffect(() => {
    if (!showNewProjModal && !showNewTaskModal) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (showNewProjModal && !isCreatingProj) setShowNewProjModal(false);
        if (showNewTaskModal && !isCreatingTask) setShowNewTaskModal(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [showNewProjModal, showNewTaskModal, isCreatingProj, isCreatingTask]);

  const handleCreateProject = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!projName.trim() || !projRootPath.trim() || isCreatingProj) return;
    setIsCreatingProj(true);
    try {
      const created = await activeApi.createProject({
        name: projName.trim(),
        root_path: projRootPath.trim(),
        kind: projKind,
      });
      addToast({ message: `Created project '${created.name}'`, type: 'success' });
      setShowNewProjModal(false);
      setProjName('');
      setProjRootPath('');
      await fetchProjects();
      setActiveProject(created);
    } catch (err: any) {
      addToast({ message: err.response?.data?.error || err.message || 'Project creation failed', type: 'error' });
    } finally {
      setIsCreatingProj(false);
    }
  };

  const handleCreateTask = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeProject || !taskGoal.trim() || isCreatingTask) return;
    setIsCreatingTask(true);
    try {
      const created = await activeApi.createTask(activeProject.id, {
        goal: taskGoal.trim(),
        acceptance_criteria: taskAcceptance.trim() || undefined,
      });
      addToast({ message: `Created task #${created.id.slice(0, 8)}`, type: 'success' });
      setShowNewTaskModal(false);
      setTaskGoal('');
      setTaskAcceptance('');
      await fetchTasks();
      setActiveTask(created);
    } catch (err: any) {
      addToast({ message: err.response?.data?.error || err.message || 'Task creation failed', type: 'error' });
    } finally {
      setIsCreatingTask(false);
    }
  };

  return (
    <div className="flex h-full bg-slate-950 text-slate-100 overflow-hidden">
      {/* Sidebar: Projects & Tasks */}
      <div className="w-80 border-r border-slate-800 bg-slate-900 flex flex-col">
        {/* Projects Section */}
        <div className="p-4 border-b border-slate-800 flex items-center justify-between">
          <h2 className="text-xs font-bold uppercase tracking-wider text-slate-400 flex items-center gap-2">
            <FolderGit2 size={16} aria-hidden="true" /> Projects ({projects.length})
          </h2>
          <button
            onClick={() => setShowNewProjModal(true)}
            aria-label="New Project"
            title="Create a new project"
            className="p-1 rounded bg-indigo-600 hover:bg-indigo-500 text-white text-xs flex items-center gap-1 font-bold focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:outline-none"
          >
            <Plus size={14} aria-hidden="true" /> New
          </button>
        </div>

        {/* Project Selector List */}
        <div className="max-h-48 overflow-y-auto p-2 border-b border-slate-800 space-y-1">
          {projects.length === 0 ? (
            <p className="text-xs text-slate-500 p-2 italic">No projects created yet.</p>
          ) : (
            projects.map((p: Project) => (
              <button
                key={p.id}
                onClick={() => {
                  setActiveProject(p);
                  setActiveTask(null);
                }}
                aria-current={activeProject?.id === p.id ? 'true' : undefined}
                title={`Select project ${p.name}`}
                className={`w-full text-left p-2.5 rounded-lg text-xs transition flex flex-col gap-0.5 focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:outline-none ${
                  activeProject?.id === p.id ? 'bg-indigo-600/20 text-indigo-300 border border-indigo-500/30' : 'text-slate-300 hover:bg-slate-800'
                }`}
              >
                <div className="flex items-center justify-between font-bold">
                  <span>{p.name}</span>
                  <span className="text-[10px] uppercase px-1.5 py-0.2 rounded bg-slate-800 text-slate-400">{p.kind}</span>
                </div>
                <span className="text-[10px] text-slate-500 font-mono truncate">{p.root_path}</span>
              </button>
            ))
          )}
        </div>

        {/* Tasks Section for Active Project */}
        {activeProject && (
          <div className="flex-1 flex flex-col min-h-0">
            <div className="p-4 border-b border-slate-800 flex items-center justify-between">
              <h3 className="text-xs font-bold uppercase tracking-wider text-slate-400">Tasks ({tasks.length})</h3>
              <button
                onClick={() => setShowNewTaskModal(true)}
                aria-label="New Task"
                title="Create a new task for this project"
                className="p-1 rounded bg-indigo-600 hover:bg-indigo-500 text-white text-xs flex items-center gap-1 font-bold focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:outline-none"
              >
                <Plus size={14} aria-hidden="true" /> Task
              </button>
            </div>

            <div className="flex-1 overflow-y-auto p-2 space-y-1">
              {tasks.length === 0 ? (
                <p className="text-xs text-slate-500 p-2 italic">No tasks created for this project.</p>
              ) : (
                tasks.map((t: Task) => (
                  <button
                    key={t.id}
                    onClick={() => setActiveTask(t)}
                    aria-current={activeTask?.id === t.id ? 'true' : undefined}
                    title={`Select task #${t.id.slice(0, 8)}: ${t.goal}`}
                    className={`w-full text-left p-3 rounded-lg text-xs transition flex flex-col gap-1.5 focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:outline-none ${
                      t.parent_id ? 'ml-3 border-l-2 border-indigo-500/50 pl-2.5' : ''
                    } ${activeTask?.id === t.id ? 'bg-indigo-600/20 text-indigo-300 border border-indigo-500/30' : 'text-slate-300 hover:bg-slate-800'}`}
                  >
                    <div className="flex items-center justify-between">
                      <span
                        className={`text-[10px] font-bold uppercase px-1.5 py-0.5 rounded ${
                          t.status === 'needs_review'
                            ? 'bg-amber-500/20 text-amber-400 border border-amber-500/30'
                            : t.status === 'running'
                            ? 'bg-indigo-500/20 text-indigo-400 border border-indigo-500/30'
                            : t.status === 'accepted'
                            ? 'bg-green-500/20 text-green-400 border border-green-500/30'
                            : 'bg-slate-800 text-slate-400'
                        }`}
                      >
                        {t.status}
                      </span>
                      <span className="text-[10px] text-slate-500 font-mono">#{t.id.slice(0, 8)}</span>
                    </div>
                    <p className="font-medium line-clamp-2">{t.goal}</p>
                  </button>
                ))
              )}
            </div>
          </div>
        )}
      </div>

      {/* Main Panel: Task Detail or Empty State */}
      <div className="flex-1 min-h-0 bg-slate-950">
        {activeTask ? (
          <TaskView task={activeTask} api={activeApi} onRefresh={fetchTasks} />
        ) : (
          <div className="h-full flex flex-col items-center justify-center text-slate-500 space-y-3 p-6 text-center">
            <FolderGit2 size={48} className="text-slate-700" aria-hidden="true" />
            <h3 className="text-lg font-bold text-slate-300">Select a Project & Task</h3>
            <p className="text-xs max-w-md text-slate-500">
              Ghostlink Task Runtime lets an agent execute against your workspace, run tests, and propose a ReviewPacket for human acceptance.
            </p>
          </div>
        )}
      </div>

      {/* New Project Modal */}
      {showNewProjModal && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <form
            onSubmit={handleCreateProject}
            role="dialog"
            aria-modal="true"
            aria-labelledby="new-proj-modal-title"
            className="bg-slate-900 border border-slate-800 rounded-xl p-6 w-full max-w-md space-y-4 shadow-2xl"
          >
            <h3 id="new-proj-modal-title" className="text-base font-bold text-slate-100">Create New Project</h3>
            <div>
              <label htmlFor="proj-name-input" className="block text-xs text-slate-400 mb-1">Project Name</label>
              <input
                id="proj-name-input"
                type="text"
                required
                value={projName}
                onChange={(e) => setProjName(e.target.value)}
                placeholder="e.g. My Backend Crate"
                className="w-full px-3 py-2 bg-slate-950 border border-slate-700 rounded-lg text-xs text-slate-200 focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
            </div>
            <div>
              <label htmlFor="proj-root-input" className="block text-xs text-slate-400 mb-1">Root Path (Absolute or relative path)</label>
              <input
                id="proj-root-input"
                type="text"
                required
                value={projRootPath}
                onChange={(e) => setProjRootPath(e.target.value)}
                placeholder="e.g. /home/user/my-project or ."
                className="w-full px-3 py-2 bg-slate-950 border border-slate-700 rounded-lg text-xs text-slate-200 focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
            </div>
            <div className="flex gap-2 justify-end pt-2">
              <button
                type="button"
                onClick={() => setShowNewProjModal(false)}
                disabled={isCreatingProj}
                aria-label="Cancel project creation"
                title="Cancel project creation"
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs rounded-lg font-bold disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:outline-none"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={isCreatingProj}
                aria-busy={isCreatingProj}
                aria-label={isCreatingProj ? 'Creating project...' : 'Create project'}
                title={isCreatingProj ? 'Creating project...' : 'Create project'}
                className="px-4 py-2 bg-indigo-600 hover:bg-indigo-500 text-white text-xs rounded-lg font-bold flex items-center gap-1.5 disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:outline-none"
              >
                {isCreatingProj && <Loader2 size={14} className="animate-spin" aria-hidden="true" />}
                {isCreatingProj ? 'Creating...' : 'Create Project'}
              </button>
            </div>
          </form>
        </div>
      )}

      {/* New Task Modal */}
      {showNewTaskModal && (
        <div className="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <form
            onSubmit={handleCreateTask}
            role="dialog"
            aria-modal="true"
            aria-labelledby="new-task-modal-title"
            className="bg-slate-900 border border-slate-800 rounded-xl p-6 w-full max-w-md space-y-4 shadow-2xl"
          >
            <h3 id="new-task-modal-title" className="text-base font-bold text-slate-100">Create Task for {activeProject?.name}</h3>
            <div>
              <label htmlFor="task-goal-input" className="block text-xs text-slate-400 mb-1">Task Goal</label>
              <textarea
                id="task-goal-input"
                required
                rows={3}
                value={taskGoal}
                onChange={(e) => setTaskGoal(e.target.value)}
                placeholder="e.g. Find timeout handling, propose a fix, run relevant tests"
                className="w-full px-3 py-2 bg-slate-950 border border-slate-700 rounded-lg text-xs text-slate-200 focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
            </div>
            <div>
              <label htmlFor="task-criteria-input" className="block text-xs text-slate-400 mb-1">Acceptance Criteria (Optional)</label>
              <input
                id="task-criteria-input"
                type="text"
                value={taskAcceptance}
                onChange={(e) => setTaskAcceptance(e.target.value)}
                placeholder="e.g. Tests pass with no regressions"
                className="w-full px-3 py-2 bg-slate-950 border border-slate-700 rounded-lg text-xs text-slate-200 focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
            </div>
            <div className="flex gap-2 justify-end pt-2">
              <button
                type="button"
                onClick={() => setShowNewTaskModal(false)}
                disabled={isCreatingTask}
                aria-label="Cancel task creation"
                title="Cancel task creation"
                className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs rounded-lg font-bold disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:outline-none"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={isCreatingTask}
                aria-busy={isCreatingTask}
                aria-label={isCreatingTask ? 'Creating task...' : 'Create task'}
                title={isCreatingTask ? 'Creating task...' : 'Create task'}
                className="px-4 py-2 bg-indigo-600 hover:bg-indigo-500 text-white text-xs rounded-lg font-bold flex items-center gap-1.5 disabled:opacity-50 focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:outline-none"
              >
                {isCreatingTask && <Loader2 size={14} className="animate-spin" aria-hidden="true" />}
                {isCreatingTask ? 'Creating...' : 'Create Task'}
              </button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
};

import { createRouter, createWebHistory } from "vue-router";
import Home from "./pages/Home.vue";
import TaskList from "./pages/TaskList.vue";
import TaskDetail from "./pages/TaskDetail.vue";
import CreateTask from "./pages/CreateTask.vue";
import Login from "./pages/Login.vue";
import Settings from "./pages/Settings.vue";
import BugRoute from "./pages/BugRoute.vue";

export const router = createRouter({
	history: createWebHistory(),
	routes: [
		{ path: "/", component: Home },
		{ path: "/tasks", component: TaskList },
		{ path: "/tasks/new", component: CreateTask },
		{ path: "/tasks/:id", component: TaskDetail },
		{ path: "/login", component: Login },
		{ path: "/settings", component: Settings },
		{ path: "/bugs/:name", component: BugRoute },
	],
});

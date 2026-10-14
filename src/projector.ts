import { mount } from "svelte";
import "./styles/theme.css";
import Projector from "./components/projector/Projector.svelte";

const app = mount(Projector, { target: document.getElementById("app")! });

export default app;

import { mount } from "svelte";
import Root from "./Root.svelte";
import "./style.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing app root");
mount(Root, { target: root });

import type { ReactNode } from "react";
import { ArchiveRestore, BarChart3, Bot, Cog, HardDrive, Inbox, Search, Workflow, Cloud, Code2, Coffee, Compass, Cpu, Feather as FeatherIcon, Flame, Gauge, Leaf, Network, SquareTerminal, Terminal, Database, DatabaseZap, Feather, Layers3, Mail, Package, Radio, Server, ShieldCheck, Table2, TableProperties, Zap } from "lucide-react";
import type { Runtime } from "../lib/api";

/** Icon and colour used for each runtime across the app. */
export const runtimeTiles: Record<Runtime, { className: string; text: string; icon: (size: number) => ReactNode }> = {
  apache: {
    className: "bg-rose-500/10 text-rose-500",
    text: "text-rose-500",
    icon: (size) => <Feather size={size} strokeWidth={1.75} />,
  },
  nginx: {
    className: "bg-emerald-600/10 text-emerald-600",
    text: "text-emerald-600",
    icon: (size) => <Server size={size} strokeWidth={1.75} />,
  },
  caddy: {
    className: "bg-cyan-600/10 text-cyan-600 dark:text-cyan-400",
    text: "text-cyan-600 dark:text-cyan-400",
    icon: (size) => <ShieldCheck size={size} strokeWidth={1.75} />,
  },
  frankenphp: {
    className: "bg-violet-600/10 text-violet-600 dark:text-violet-400",
    text: "text-violet-600 dark:text-violet-400",
    icon: (size) => <Flame size={size} strokeWidth={1.75} />,
  },
  tomcat: {
    className: "bg-yellow-600/10 text-yellow-700 dark:text-yellow-400",
    text: "text-yellow-700 dark:text-yellow-400",
    icon: (size) => <Cpu size={size} strokeWidth={1.75} />,
  },
  java: {
    className: "bg-orange-600/10 text-orange-600 dark:text-orange-400",
    text: "text-orange-600 dark:text-orange-400",
    icon: (size) => <Coffee size={size} strokeWidth={1.75} />,
  },
  php: {
    className: "bg-indigo-500/10 text-indigo-500",
    text: "text-indigo-500",
    icon: (size) => <Code2 size={size} strokeWidth={1.75} />,
  },
  mariadb: {
    className: "bg-amber-600/12 text-amber-600",
    text: "text-amber-600",
    icon: (size) => <Database size={size} strokeWidth={1.75} />,
  },
  mongodb: {
    className: "bg-green-600/10 text-green-700 dark:text-green-400",
    text: "text-green-700 dark:text-green-400",
    icon: (size) => <Leaf size={size} strokeWidth={1.75} />,
  },
  meilisearch: {
    className: "bg-pink-500/10 text-pink-600 dark:text-pink-400",
    text: "text-pink-600 dark:text-pink-400",
    icon: (size) => <Search size={size} strokeWidth={1.75} />,
  },
  typesense: {
    className: "bg-violet-500/10 text-violet-600 dark:text-violet-400",
    text: "text-violet-600 dark:text-violet-400",
    icon: (size) => <Search size={size} strokeWidth={1.75} />,
  },
  rustfs: {
    className: "bg-orange-600/10 text-orange-700 dark:text-orange-400",
    text: "text-orange-700 dark:text-orange-400",
    icon: (size) => <HardDrive size={size} strokeWidth={1.75} />,
  },
  minio: {
    className: "bg-red-600/10 text-red-700 dark:text-red-400",
    text: "text-red-700 dark:text-red-400",
    icon: (size) => <HardDrive size={size} strokeWidth={1.75} />,
  },
  elasticsearch: {
    className: "bg-teal-500/10 text-teal-600 dark:text-teal-400",
    text: "text-teal-600 dark:text-teal-400",
    icon: (size) => <Search size={size} strokeWidth={1.75} />,
  },
  opensearch: {
    className: "bg-blue-500/10 text-blue-600 dark:text-blue-400",
    text: "text-blue-600 dark:text-blue-400",
    icon: (size) => <Search size={size} strokeWidth={1.75} />,
  },
  n8n: {
    className: "bg-rose-500/10 text-rose-600 dark:text-rose-400",
    text: "text-rose-600 dark:text-rose-400",
    icon: (size) => <Workflow size={size} strokeWidth={1.75} />,
  },
  rabbitmq: {
    className: "bg-orange-500/10 text-orange-600 dark:text-orange-400",
    text: "text-orange-600 dark:text-orange-400",
    icon: (size) => <Inbox size={size} strokeWidth={1.75} />,
  },
  go: {
    className: "bg-cyan-500/10 text-cyan-600 dark:text-cyan-400",
    text: "text-cyan-600 dark:text-cyan-400",
    icon: (size) => <Code2 size={size} strokeWidth={1.75} />,
  },
  rust: {
    className: "bg-orange-700/10 text-orange-800 dark:text-orange-400",
    text: "text-orange-800 dark:text-orange-400",
    icon: (size) => <Cog size={size} strokeWidth={1.75} />,
  },
  maven: {
    className: "bg-red-700/10 text-red-700 dark:text-red-400",
    text: "text-red-700 dark:text-red-400",
    icon: (size) => <Package size={size} strokeWidth={1.75} />,
  },
  gradle: {
    className: "bg-teal-600/10 text-teal-700 dark:text-teal-400",
    text: "text-teal-700 dark:text-teal-400",
    icon: (size) => <Layers3 size={size} strokeWidth={1.75} />,
  },
  erlang: {
    className: "bg-red-500/10 text-red-600 dark:text-red-400",
    text: "text-red-600 dark:text-red-400",
    icon: (size) => <Cog size={size} strokeWidth={1.75} />,
  },
  ollama: {
    className: "bg-zinc-500/15 text-fg",
    text: "text-fg",
    icon: (size) => <Bot size={size} strokeWidth={1.75} />,
  },
  mongotools: {
    className: "bg-green-600/10 text-green-700 dark:text-green-400",
    text: "text-green-700 dark:text-green-400",
    icon: (size) => <ArchiveRestore size={size} strokeWidth={1.75} />,
  },
  compass: {
    className: "bg-green-600/10 text-green-700 dark:text-green-400",
    text: "text-green-700 dark:text-green-400",
    icon: (size) => <Compass size={size} strokeWidth={1.75} />,
  },
  sqlite: {
    className: "bg-sky-600/10 text-sky-700 dark:text-sky-400",
    text: "text-sky-700 dark:text-sky-400",
    icon: (size) => <FeatherIcon size={size} strokeWidth={1.75} />,
  },
  clickhouse: {
    className: "bg-yellow-500/15 text-yellow-700 dark:text-yellow-300",
    text: "text-yellow-700 dark:text-yellow-300",
    icon: (size) => <BarChart3 size={size} strokeWidth={1.75} />,
  },
  neo4j: {
    className: "bg-cyan-600/10 text-cyan-700 dark:text-cyan-400",
    text: "text-cyan-700 dark:text-cyan-400",
    icon: (size) => <Network size={size} strokeWidth={1.75} />,
  },
  mongosh: {
    className: "bg-green-600/10 text-green-700 dark:text-green-400",
    text: "text-green-700 dark:text-green-400",
    icon: (size) => <SquareTerminal size={size} strokeWidth={1.75} />,
  },
  python: {
    className: "bg-blue-500/10 text-blue-600 dark:text-blue-400",
    text: "text-blue-600 dark:text-blue-400",
    icon: (size) => <Terminal size={size} strokeWidth={1.75} />,
  },
  phppgadmin: {
    className: "bg-sky-600/10 text-sky-700 dark:text-sky-400",
    text: "text-sky-700 dark:text-sky-400",
    icon: (size) => <TableProperties size={size} strokeWidth={1.75} />,
  },
  pgadmin: {
    className: "bg-indigo-600/10 text-indigo-700 dark:text-indigo-400",
    text: "text-indigo-700 dark:text-indigo-400",
    icon: (size) => <Gauge size={size} strokeWidth={1.75} />,
  },
  postgres: {
    className: "bg-sky-700/10 text-sky-700 dark:text-sky-400",
    text: "text-sky-700 dark:text-sky-400",
    icon: (size) => <Database size={size} strokeWidth={1.75} />,
  },
  mysql: {
    className: "bg-blue-600/10 text-blue-600 dark:text-blue-400",
    text: "text-blue-600 dark:text-blue-400",
    icon: (size) => <DatabaseZap size={size} strokeWidth={1.75} />,
  },
  mailpit: {
    className: "bg-sky-500/10 text-sky-500",
    text: "text-sky-500",
    icon: (size) => <Mail size={size} strokeWidth={1.75} />,
  },
  adminer: {
    className: "bg-teal-500/10 text-teal-500",
    text: "text-teal-500",
    icon: (size) => <Table2 size={size} strokeWidth={1.75} />,
  },
  redis: {
    className: "bg-red-500/10 text-red-500",
    text: "text-red-500",
    icon: (size) => <Zap size={size} strokeWidth={1.75} />,
  },
  ngrok: {
    className: "bg-violet-500/10 text-violet-500",
    text: "text-violet-500",
    icon: (size) => <Radio size={size} strokeWidth={1.75} />,
  },
  cloudflared: {
    className: "bg-orange-400/15 text-orange-500",
    text: "text-orange-500",
    icon: (size) => <Cloud size={size} strokeWidth={1.75} />,
  },
  composer: {
    className: "bg-yellow-700/10 text-yellow-700",
    text: "text-yellow-700",
    icon: (size) => <Package size={size} strokeWidth={1.75} />,
  },
  memcached: {
    className: "bg-teal-500/10 text-teal-600",
    text: "text-teal-600",
    icon: (size) => <Layers3 size={size} strokeWidth={1.75} />,
  },
  phpredisadmin: {
    className: "bg-red-500/10 text-red-600 dark:text-red-400",
    text: "text-red-600 dark:text-red-400",
    icon: (size) => <TableProperties size={size} strokeWidth={1.75} />,
  },
  phpmyadmin: {
    className: "bg-orange-500/10 text-orange-500",
    text: "text-orange-500",
    icon: (size) => <TableProperties size={size} strokeWidth={1.75} />,
  },
};

/** A runtime's tile, or a plain one for a runtime the backend added that has none here yet. */
export function tileFor(id: string): { className: string; text: string; icon: (size: number) => ReactNode } {
  return (
    runtimeTiles[id as Runtime] ?? {
      className: "bg-surface-2 text-muted",
      text: "text-muted",
      icon: (size: number) => <Package size={size} strokeWidth={1.75} />,
    }
  );
}

import { redirect } from "next/navigation";
import { getSessionToken } from "@/lib/api";

export default async function HomePage() {
  redirect((await getSessionToken()) ? "/dashboard" : "/login");
}

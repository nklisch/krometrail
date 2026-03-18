import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import type { CDPClient } from "../recorder/cdp-client.js";
import type { RecordedEvent } from "../types.js";
import type { BrowserDatabase } from "./database.js";

const MAX_BODY_SIZE = 10 * 1024 * 1024; // 10MB

export class NetworkExtractor {
	/**
	 * Extract and store network bodies for network events.
	 * Called during marker-triggered persistence.
	 */
	async extractBodies(events: RecordedEvent[], cdpClient: CDPClient, tabSessionId: string, networkDir: string, db: BrowserDatabase, sessionId: string): Promise<void> {
		const networkEvents = events.filter((e) => e.type === "network_request" || e.type === "network_response");

		for (const event of networkEvents) {
			const requestId = event.data.requestId as string | undefined;
			if (!requestId) continue;

			try {
				if (event.type === "network_response" && event.data.hasBody) {
					const result = (await cdpClient.sendToTarget(tabSessionId, "Network.getResponseBody", { requestId })) as {
						body: string;
						base64Encoded: boolean;
					};

					const content = result.base64Encoded ? Buffer.from(result.body, "base64") : Buffer.from(result.body, "utf-8");

					const truncated = content.length > MAX_BODY_SIZE ? content.subarray(0, MAX_BODY_SIZE) : content;

					const fileName = `res_${requestId}_body.bin`;
					const filePath = resolve(networkDir, fileName);
					writeFileSync(filePath, truncated);

					db.insertNetworkBody({
						eventId: event.id,
						sessionId,
						responseBodyPath: fileName,
						responseSize: truncated.length,
						contentType: event.data.mimeType as string | undefined,
					});
				}

				if (event.type === "network_request" && event.data.postData) {
					const fileName = `req_${requestId}_body.bin`;
					const filePath = resolve(networkDir, fileName);
					writeFileSync(filePath, event.data.postData as string);

					const headers = event.data.headers as Record<string, string> | undefined;
					const requestContentType = headers?.["Content-Type"] ?? headers?.["content-type"];

					db.insertNetworkBody({
						eventId: event.id,
						sessionId,
						requestBodyPath: fileName,
						requestContentType,
					});
				}
			} catch {
				// Body may not be available (e.g., request was cancelled, tab navigated away)
			}
		}
	}
}

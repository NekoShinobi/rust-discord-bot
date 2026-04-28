<script lang="ts">
	import { onMount } from 'svelte';

	interface UserStatus {
		user_name: string;
		last_response: number;
		enrolled_at: number;
		death_count: number;
		last_alive_start: number;
		alive_days: number;
		is_dead: boolean;
	}

	interface AydyState {
		channel_key: string;
		channel_id: number;
		guild_id: number | null;
		message_id: number | null;
		last_sent: number;
		enrolled_users: Record<string, UserStatus>;
	}

	interface AydyResponse {
		aydy: AydyState[];
		count: number;
	}

	let aydyStates = $state<AydyState[]>([]);
	let isLoading = $state(true);
	let error = $state('');
	let expandedChannel = $state<string | null>(null);

	async function fetchAydy() {
		isLoading = true;
		error = '';
		try {
			const response = await fetch('/api/aydy');
			if (!response.ok) {
				throw new Error(`HTTP error! status: ${response.status}`);
			}
			const data: AydyResponse = await response.json();
			aydyStates = data.aydy;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to fetch AYDY data';
			console.error('Error fetching AYDY data:', e);
		} finally {
			isLoading = false;
		}
	}

	function toggleExpand(channelKey: string) {
		expandedChannel = expandedChannel === channelKey ? null : channelKey;
	}

	function formatTimestamp(timestamp: number): string {
		return new Date(timestamp * 1000).toLocaleString();
	}

	function getRelativeTime(timestamp: number): string {
		const now = Math.floor(Date.now() / 1000);
		const diff = now - timestamp;
		const hours = Math.floor(diff / 3600);
		const days = Math.floor(hours / 24);

		if (days > 0) {
			return `${days} day${days !== 1 ? 's' : ''} ago`;
		} else if (hours > 0) {
			return `${hours} hour${hours !== 1 ? 's' : ''} ago`;
		} else {
			const minutes = Math.floor(diff / 60);
			return `${minutes} minute${minutes !== 1 ? 's' : ''} ago`;
		}
	}

	function getNonResponders(state: AydyState, hours: number): Array<[string, UserStatus]> {
		const cutoff = Math.floor(Date.now() / 1000) - hours * 3600;
		return Object.entries(state.enrolled_users).filter(
			([_, user]) => user.last_response < cutoff
		);
	}

	function getUserCount(state: AydyState): number {
		return Object.keys(state.enrolled_users).length;
	}

	function getSortedUsers(state: AydyState): Array<[string, UserStatus]> {
		return Object.entries(state.enrolled_users).sort((a, b) => {
			// Sort by alive_days descending (longest alive streaks first)
			return (b[1].alive_days || 0) - (a[1].alive_days || 0);
		});
	}

	onMount(() => {
		fetchAydy();
		// Auto-refresh every 60 seconds
		const interval = setInterval(fetchAydy, 60000);
		return () => clearInterval(interval);
	});
</script>

<div class="max-w-7xl mx-auto">
	<div class="mb-8">
		<h1 class="text-4xl font-bold text-white mb-2">AYDY (Are You Dead Yet?)</h1>
		<p class="text-gray-400 text-lg">Active AYDY checks across all channels</p>
	</div>

	{#if error}
		<div class="bg-red-500/10 border border-red-500/50 text-red-400 rounded-lg p-4 mb-6">
			<strong>Error:</strong>
			{error}
		</div>
	{/if}

	{#if isLoading}
		<div class="flex items-center justify-center py-12">
			<div class="animate-spin rounded-full h-12 w-12 border-b-2 border-white"></div>
		</div>
	{:else if aydyStates.length === 0}
		<div class="bg-gray-800/50 backdrop-blur-sm rounded-lg p-8 text-center border border-gray-700/50">
			<p class="text-gray-400 text-lg">No active AYDY checks found</p>
			<p class="text-gray-500 text-sm mt-2">Use <code class="bg-gray-700/50 px-2 py-1 rounded">/aydy start</code> in a Discord channel to begin</p>
		</div>
	{:else}
		<div class="space-y-4">
			{#each aydyStates as state (state.channel_key)}
				{@const nonResponders48h = getNonResponders(state, 48)}
				{@const userCount = getUserCount(state)}
				<div class="bg-gray-800/50 backdrop-blur-sm rounded-lg border border-gray-700/50 overflow-hidden">
					<div class="p-4 flex items-center justify-between cursor-pointer hover:bg-gray-700/30 transition-colors" onclick={() => toggleExpand(state.channel_key)}>
						<div class="flex-1">
							<div class="flex items-center gap-4">
								<div>
									<h3 class="text-lg font-semibold text-white">
										Channel: <code class="text-blue-400">{state.channel_id}</code>
									</h3>
									{#if state.guild_id}
										<p class="text-sm text-gray-400">Guild: <code class="text-purple-400">{state.guild_id}</code></p>
									{/if}
								</div>
							</div>
							<div class="mt-2 flex gap-6 text-sm">
								<div class="text-gray-400">
									<span class="text-white font-medium">{userCount}</span> enrolled user{userCount !== 1 ? 's' : ''}
								</div>
								<div class="text-gray-400">
									<span class="text-{nonResponders48h.length > 0 ? 'red' : 'green'}-400 font-medium">{nonResponders48h.length}</span> non-responder{nonResponders48h.length !== 1 ? 's' : ''} (48h)
								</div>
								<div class="text-gray-400">
									Last sent: <span class="text-gray-300">{getRelativeTime(state.last_sent)}</span>
								</div>
							</div>
						</div>
						<div class="text-gray-400">
							<svg
								class="w-6 h-6 transition-transform {expandedChannel === state.channel_key ? 'rotate-180' : ''}"
								fill="none"
								stroke="currentColor"
								viewBox="0 0 24 24"
							>
								<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"></path>
							</svg>
						</div>
					</div>

					{#if expandedChannel === state.channel_key}
						<div class="border-t border-gray-700/50 p-4 bg-gray-900/30">
							<div class="mb-6">
								<h4 class="text-sm font-semibold text-gray-300 mb-3">Details</h4>
								<div class="grid grid-cols-2 gap-4 text-sm">
									<div>
										<span class="text-gray-400">Channel ID:</span>
										<code class="ml-2 text-blue-400">{state.channel_id}</code>
									</div>
									{#if state.guild_id}
										<div>
											<span class="text-gray-400">Guild ID:</span>
											<code class="ml-2 text-purple-400">{state.guild_id}</code>
										</div>
									{/if}
									{#if state.message_id}
										<div>
											<span class="text-gray-400">Message ID:</span>
											<code class="ml-2 text-gray-300">{state.message_id}</code>
										</div>
									{/if}
									<div>
										<span class="text-gray-400">Last Sent:</span>
										<span class="ml-2 text-gray-300">{formatTimestamp(state.last_sent)}</span>
									</div>
								</div>
							</div>

							{#if userCount > 0}
								<div class="mb-4">
									<h4 class="text-sm font-semibold text-gray-300 mb-3">
										Enrolled Users ({userCount})
									</h4>
									<div class="overflow-x-auto">
										<table class="w-full text-sm">
											<thead class="bg-gray-800/50 border-b border-gray-700">
												<tr>
													<th class="px-4 py-2 text-left text-gray-300 font-medium">Username</th>
													<th class="px-4 py-2 text-left text-gray-300 font-medium">Alive Streak</th>
													<th class="px-4 py-2 text-left text-gray-300 font-medium">Deaths</th>
													<th class="px-4 py-2 text-left text-gray-300 font-medium">Last Response</th>
													<th class="px-4 py-2 text-left text-gray-300 font-medium">Status</th>
												</tr>
											</thead>
											<tbody class="divide-y divide-gray-700/50">
												{#each getSortedUsers(state) as [userId, user], index}
													{@const hoursSinceResponse = Math.floor((Date.now() / 1000 - user.last_response) / 3600)}
													{@const isDead = user.is_dead ?? (hoursSinceResponse >= 48)}
													<tr class="hover:bg-gray-700/20 transition-colors {isDead ? 'bg-red-900/10' : ''}">
														<td class="px-4 py-3">
															<div class="flex items-center gap-2">
																{#if index < 3 && !isDead}
																	<span class="text-xl">{index === 0 ? '🥇' : index === 1 ? '🥈' : '🥉'}</span>
																{/if}
																<div>
																	<div class="text-white font-medium">{user.user_name}</div>
																	<code class="text-blue-400 text-xs">{userId}</code>
																</div>
															</div>
														</td>
														<td class="px-4 py-3">
															<div class="text-white font-semibold">
																{user.alive_days?.toFixed(1) ?? '0.0'} days
															</div>
															<span class="text-gray-500 text-xs">Since {formatTimestamp(user.last_alive_start)}</span>
														</td>
														<td class="px-4 py-3">
															{#if user.death_count > 0}
																<span class="inline-flex items-center px-2 py-1 rounded-full text-sm font-medium bg-red-500/20 text-red-400 border border-red-500/30">
																	💀 {user.death_count}
																</span>
															{:else}
																<span class="text-gray-500">—</span>
															{/if}
														</td>
														<td class="px-4 py-3 text-gray-300">
															{formatTimestamp(user.last_response)}
															<span class="text-gray-500 text-xs block">{getRelativeTime(user.last_response)}</span>
														</td>
														<td class="px-4 py-3">
															{#if isDead}
																<span class="inline-flex items-center px-2 py-1 rounded-full text-xs font-medium bg-red-500/20 text-red-400 border border-red-500/30">
																	⚠️ Dead (48h+)
																</span>
															{:else if hoursSinceResponse >= 24}
																<span class="inline-flex items-center px-2 py-1 rounded-full text-xs font-medium bg-yellow-500/20 text-yellow-400 border border-yellow-500/30">
																	⏰ {hoursSinceResponse}h ago
																</span>
															{:else}
																<span class="inline-flex items-center px-2 py-1 rounded-full text-xs font-medium bg-green-500/20 text-green-400 border border-green-500/30">
																	✓ Alive
																</span>
															{/if}
														</td>
													</tr>
												{/each}
											</tbody>
										</table>
									</div>
								</div>
							{:else}
								<div class="text-center py-6 text-gray-500">
									No enrolled users yet
								</div>
							{/if}
						</div>
					{/if}
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	code {
		font-family: var(--font-family-mono);
		font-size: 0.9em;
	}
</style>

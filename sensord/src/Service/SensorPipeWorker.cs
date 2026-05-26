using System.Text;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

namespace Sensord.Service;

/// <summary>
/// Long-running service loop. Accepts one client at a time on the
/// <see cref="PipeServer"/>, writes one NDJSON line per tick, and tears the
/// pipe down + reopens it when the client disconnects. The poll loop is
/// suspended while no client is connected (no point computing snapshots
/// nobody is reading).
/// </summary>
internal sealed class SensorPipeWorker : BackgroundService
{
    private readonly ILogger<SensorPipeWorker> _log;
    private readonly string _pipeName;

    public SensorPipeWorker(ILogger<SensorPipeWorker> log, string pipeName)
    {
        _log = log;
        _pipeName = pipeName;
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        _log.LogInformation("SensorPipeWorker started, pipe={Pipe}", _pipeName);
        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                await using var pipe = PipeServer.Create(_pipeName);
                _log.LogInformation("Waiting for client on {Pipe}", _pipeName);
                await pipe.WaitForConnectionAsync(stoppingToken);
                _log.LogInformation("Client connected");
                await ServeOne(pipe, stoppingToken);
            }
            catch (OperationCanceledException)
            {
                break;
            }
            catch (Exception ex)
            {
                _log.LogError(ex, "pipe loop error");
                try { await Task.Delay(1000, stoppingToken); }
                catch (OperationCanceledException) { break; }
            }
        }
    }

    private static async Task ServeOne(Stream pipe, CancellationToken token)
    {
        using var writer = new StreamWriter(
            pipe, new UTF8Encoding(false), bufferSize: 1024, leaveOpen: true)
        {
            AutoFlush = true,
            NewLine = "\n"
        };
        long tick = 0;
        while (!token.IsCancellationRequested)
        {
            try
            {
                await writer.WriteLineAsync($"{{\"heartbeat\":{tick}}}");
                tick++;
                await Task.Delay(1000, token);
            }
            catch (IOException)
            {
                // Client disconnected; outer loop will accept the next one.
                return;
            }
        }
    }
}

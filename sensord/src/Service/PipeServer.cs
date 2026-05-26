using System.IO.Pipes;
using System.Security.AccessControl;
using System.Security.Principal;

namespace Sensord.Service;

/// <summary>
/// Owns one <see cref="NamedPipeServerStream"/> at a time. Builds it with an
/// explicit ACL so the interactive (non-elevated) user can connect even though
/// the service itself runs as LocalSystem. Single-client by design
/// (<c>MaxAllowedServerInstances = 1</c>): a second concurrent dashboard
/// receives <c>ERROR_PIPE_BUSY</c>.
/// </summary>
internal static class PipeServer
{
    public const string DefaultPipeName = "PerfWindowSensor";

    public static NamedPipeServerStream Create(string pipeName)
    {
        var security = new PipeSecurity();
        // Authenticated Users (well-known SID S-1-5-11) — covers any
        // interactive user including the one running the dashboard.
        var authUsers = new SecurityIdentifier(WellKnownSidType.AuthenticatedUserSid, null);
        security.AddAccessRule(new PipeAccessRule(
            authUsers,
            PipeAccessRights.ReadWrite | PipeAccessRights.CreateNewInstance,
            AccessControlType.Allow));

        return NamedPipeServerStreamAcl.Create(
            pipeName,
            PipeDirection.InOut,
            maxNumberOfServerInstances: 1,
            transmissionMode: PipeTransmissionMode.Byte,
            options: PipeOptions.Asynchronous,
            inBufferSize: 4096,
            outBufferSize: 64 * 1024,
            pipeSecurity: security);
    }
}

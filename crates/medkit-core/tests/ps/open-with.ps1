# For the Windows tests: the programs Windows offers under "Open with" for
# each extension given, as the shell itself lists them (SHAssocEnumHandlers,
# every handler). One line per handler: "<ext> | <display name> | <name>".
# Read-only.

param([string[]]$Extensions)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

namespace MedkitTest
{
    // Only the first two methods are declared: they are the only ones called,
    // and their vtable slots do not depend on the rest.
    [ComImport, Guid("F04061AC-1659-4a3f-A954-775AA57FC083"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface IAssocHandler
    {
        [PreserveSig] int GetName([MarshalAs(UnmanagedType.LPWStr)] out string name);
        [PreserveSig] int GetUIName([MarshalAs(UnmanagedType.LPWStr)] out string name);
    }

    [ComImport, Guid("973810ae-9599-4b88-9e4d-6ee98c9552da"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface IEnumAssocHandlers
    {
        [PreserveSig] int Next(int count, [Out, MarshalAs(UnmanagedType.LPArray, ArraySubType = UnmanagedType.Interface, SizeParamIndex = 0)] IAssocHandler[] items, out int fetched);
    }

    public static class Handlers
    {
        [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
        static extern int SHAssocEnumHandlers(string extension, int filter, out IEnumAssocHandlers handlers);

        public static List<string> For(string extension)
        {
            var result = new List<string>();
            IEnumAssocHandlers list;
            int hr = SHAssocEnumHandlers(extension, 0, out list);
            if (hr != 0 || list == null)
            {
                result.Add("(error " + hr + ") | ");
                return result;
            }
            var one = new IAssocHandler[1];
            int fetched;
            while (list.Next(1, one, out fetched) == 0 && fetched == 1)
            {
                string name;
                string ui;
                if (one[0].GetName(out name) != 0) { name = ""; }
                if (one[0].GetUIName(out ui) != 0) { ui = ""; }
                result.Add(ui + " | " + name);
                Marshal.ReleaseComObject(one[0]);
            }
            Marshal.ReleaseComObject(list);
            return result;
        }
    }
}
'@

foreach ($ext in $Extensions) {
    foreach ($line in [MedkitTest.Handlers]::For($ext)) {
        '{0} | {1}' -f $ext, $line
    }
}

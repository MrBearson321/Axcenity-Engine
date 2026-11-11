// XB10k: lifts 100 drops back up every second, so some always fall.
using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class Lifter : MonoBehaviour
{
    public List<Rigidbody> drops = new List<Rigidbody>();
    int n = 1;

    IEnumerator Start()
    {
        var delay = new WaitForSeconds(1f);
        while (true)
        {
            yield return delay;
            for (int i = 0; i < 100; i++)
            {
                var drop = drops[n - 1];
                float x = (n % 10) * 3f - 13.5f;
                float z = ((n / 10) % 10) * 3f - 13.5f;
                drop.position = new Vector3(x, 40f, z);
                drop.transform.position = drop.position;
                drop.linearVelocity = Vector3.zero;
                drop.angularVelocity = Vector3.zero;
                n = n % drops.Count + 1;
            }
        }
    }
}

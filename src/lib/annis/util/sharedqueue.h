#pragma once

#include <queue>
#include <mutex>
#include <condition_variable>

#include <boost/circular_buffer.hpp>

namespace annis
{
  /**
   * This is a thread-safe queue that has a blocking pop() function.
   * The push() function is blocking as soon as the capacity is reached.
   *
   * It is possible to shutdown a queue. If a queue is shutdown, not new entries
   * can be added and as soon as the queue is empty the pop() funtion will return immediatly instead of waiting forever.
   * A shutdown can't be undone.
   */
  template<typename T>
  class SharedQueue
  {
  public:

    SharedQueue(size_t capacity = 128)
    :  isShutdown(false), queue(capacity), availableElements(0)
    {

    }

    /**
     * @brief Retrieve an item from the queue. This will block until an item is available. If the queue is empty
     * and shut-down it will return immediatly with "false" as a result.
     * @param item
     * @return "true" if an item was retrieved from the queue, false if not.
     */
    bool pop(T& item)
    {
      std::unique_lock<std::mutex> lock(queueMutex);
      while(availableElements == 0)
      {
        if(isShutdown)
        {
          // queue is empty and since it is shut down no new entries will be added.
          return false;
        }
        else
        {
          addedCondition.wait(lock);
        }
      }

      item = queue[--availableElements];

      lock.unlock();
      // make sure a waiting push() is notified that there is now some capacity left
      removedCondition.notify_one();

      return true;
    }

    void push(T&& item)
    {
      std::unique_lock<std::mutex> lock(queueMutex);

      while(!isShutdown && availableElements >= queue.capacity())
      {
        // wait until someone deleted something
        removedCondition.wait(lock);
      }

      if(!isShutdown)
      {
        queue.push_front(item);
        availableElements++;

        lock.unlock();
        addedCondition.notify_one();
      }
    }

    void shutdown()
    {
      std::unique_lock<std::mutex> lock(queueMutex);
      if(!isShutdown)
      {
        isShutdown = true;
        lock.unlock();
        addedCondition.notify_all();
        removedCondition.notify_all();
      }
    }


  private:

    bool isShutdown;
    size_t availableElements;

    boost::circular_buffer<T> queue;

    std::mutex queueMutex;
    std::condition_variable addedCondition;
    std::condition_variable removedCondition;

  };
}
